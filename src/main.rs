mod junit_xml;
mod s3;

#[macro_use]
extern crate rocket;

use crate::junit_xml::TestSuite;
use aws_sdk_s3::{types::ByteStream, Client, Region};
use rocket::{
    futures::FutureExt,
    serde::{
        json::{serde_json, Json},
        Deserialize, Serialize,
    },
    State,
};
use s3::{download_file, list_files};
use serde_xml_rs::from_str;
use std::{
    collections::HashSet,
    env::temp_dir,
    fs::File,
    io::{Read, Write},
};
use tempfile::{tempdir, tempdir_in, TempDir};
use tokio::task::spawn_blocking;

struct AppConfig {
    bucket_name: String,
}

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
struct Task<'a> {
    // Deserialize the following field as `s3KeyTestFile` instead of `s3_key_test_file`.
    #[serde(rename = "s3KeyTestFile")]
    s3_key_test_file: &'a str,
    #[serde(rename = "s3KeyProjectFile")]
    s3_key_project_file: &'a str,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct TestResult {
    rows: Vec<Row>,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct Row {
    #[serde(rename = "Test")]
    test_result: String,
    #[serde(rename = "SID")]
    student_id: String,
    #[serde(rename = "Name")]
    student_name: String,
}

fn prepend_to_file(file: &str, prefix: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = File::open(file).unwrap();
    let mut contents = String::new();
    f.read_to_string(&mut contents).unwrap();

    let mut f = File::create(file).unwrap();
    f.write_all(prefix.as_bytes()).unwrap();
    f.write_all(contents.as_bytes()).unwrap();

    Ok(())
}

fn run_tests_with_gradle(project_path: &str) {
    std::process::Command::new("./gradlew")
        .arg("test")
        .current_dir(project_path)
        .output()
        .expect("failed to execute process");
}

#[post("/", data = "<task>")]
async fn index(
    cfg: &State<AppConfig>,
    client: &State<Client>,
    task: Json<Task<'_>>,
) -> &'static str {
    // Download specified test file
    download_file(
        &client,
        &cfg.bucket_name,
        task.s3_key_test_file,
        "/tmp/tests/Test.java",
    )
    .await
    .unwrap();

    // List all the files in the project
    let files = list_files(&client, &cfg.bucket_name, task.s3_key_project_file)
        .await
        .unwrap();

    // Filter out all non .pde files
    let files: HashSet<_> = files
        .iter()
        .filter(|f| f.ends_with(".pde"))
        .map(|f| f.to_owned())
        .collect();

    // Download each of the files into the projects directory
    for file in &files {
        download_file(
            &client,
            &cfg.bucket_name,
            &file,
            &format!("/tmp/projects/{}", file),
        )
        .await
        .unwrap();
    }

    // Get the project paths
    let project_paths: HashSet<String> = files
        .iter()
        .map(|f| {
            let path = std::path::Path::new(f).to_owned();
            path.parent().unwrap().to_str().unwrap().to_owned()
        })
        .collect();

    for path in &project_paths {
        println!("{} ", path);
    }

    let mut result: TestResult = TestResult { rows: vec![] };

    // Run processing-java on each of the project paths as well as the tests using gradle
    let tasks: Vec<_> = project_paths
        .iter()
        .map(|path| {
            let path = path.to_owned();
            let student_info = std::path::Path::new(&path)
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();
            let project_name = std::path::Path::new(&path)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned();

            spawn_blocking(move || {
                let student_details: Vec<&str> = student_info.split("_").collect();
                let sid = student_details[0];
                let first_name = student_details[1];
                let last_name = student_details[2];

                let temp_dir = tempdir_in("/Users/jb/Desktop/test").unwrap().path().join(sid);
                std::fs::create_dir_all(&temp_dir).unwrap();

                // Run processing-java
                let output = std::process::Command::new("processing-java")
                    .arg("--force")
                    .arg(format!("--sketch=/tmp/projects/{path}"))
                    .arg(format!("--output=/tmp/output/{path}"))
                    .arg("--build")
                    .output()
                    .expect("failed to execute process");

                let mut test_result = String::default();

                if !output.status.success() {
                    test_result = format!("Error: {}", String::from_utf8_lossy(&output.stderr));
                }

                // Copy the java project template for this project
                std::process::Command::new("cp")
                    .arg("-r")
                    .arg("src/templates/testing-project/")
                    .arg(&temp_dir)
                    .output()
                    .expect("failed to copy testing project");


                println!(
                    "Copying /tmp/tests/Test.java to {}/src/main/java/org/example/Test.java", temp_dir.to_str().unwrap()
                );

                // Move the downloaded test file at /tmp/tests/Test.java to {temp_dir}/src/test/java/org/example/Test.java
                std::fs::copy(
                    "/tmp/tests/Test.java",
                    &temp_dir.join("src/test/java/org/example/Test.java"),
                )
                .unwrap();

                println!(
                    "Copying /tmp/output/{path}/source/{project_name}.java to {}/src/main/java/org/example/{project_name}.java", temp_dir.to_str().unwrap()
                );

                // Move the compiled {project_name}.java to {temp_dir}/src/main/java/org/example/{project_name}.java
                std::fs::copy(
                    format!("/tmp/output/{path}/source/{project_name}.java"),
                    &temp_dir.join(format!(
                        "src/main/java/org/example/{project_name}.java"
                    )),
                )
                .unwrap();

                // Add the org.example package to the file
                prepend_to_file(
                    &temp_dir.join(format!(
                        "src/main/java/org/example/{project_name}.java"
                    )).to_str().unwrap(), "package org.example;").unwrap();

                // Run the tests with gradle
                run_tests_with_gradle(temp_dir.to_str().unwrap());

                // Parse the test result xml file
                let mut f = File::open(
                    temp_dir.join("build/test-results/test/TEST-org.example.TestProject.xml")).unwrap();
                let mut contents = String::new();
                f.read_to_string(&mut contents).unwrap();
                let test_suite = from_str::<TestSuite>(&contents).unwrap();
                let total_tests: i32 = test_suite.tests.parse().unwrap();
                let failed_tests = test_suite.failures.parse::<i32>().unwrap() + test_suite.errors.parse::<i32>().unwrap();
                let passed_tests = total_tests - failed_tests;

                // Delete the temp directory
                std::fs::remove_dir_all(&temp_dir).unwrap();

                Row {
                    test_result: if test_result == "" {format!("Passed {passed_tests} / {total_tests} tests").to_owned()} else {test_result.to_owned()},
                    student_id: sid.to_owned(),
                    student_name: format!(
                        "{} {}",
                        first_name.to_owned(),
                        last_name.to_owned()
                    ),
                }
            })
        })
        .collect();

    for thread in tasks {
        result.rows.push(thread.await.unwrap());
    }

    let serialised = serde_json::to_string(&result).unwrap();
    let body = ByteStream::from(serialised.as_bytes().to_vec());

    // Get S3 assignment directory
    let assignment_dir = std::path::Path::new(task.s3_key_project_file)
        .parent()
        .unwrap()
        .to_str()
        .unwrap();

    // Upload compile_error as a json file to S3
    let resp = client
        .put_object()
        .bucket(&cfg.bucket_name)
        .key(format!("{}/Results/result.json", assignment_dir))
        .body(body)
        .send()
        .await;

    if let Err(e) = resp {
        println!("Error: {:?}", e);
        return "Error uploading file to S3";
    }

    "Done!"
}

#[launch]
fn rocket() -> _ {
    let cfg = AppConfig {
        bucket_name: "uploads-76078f4".into(),
    };

    let aws_config = aws_config::from_env()
        .region(Region::new("ap-southeast-2"))
        .load()
        .now_or_never()
        .unwrap();
    let client = Client::new(&aws_config);

    rocket::build()
        .manage(cfg)
        .manage(client)
        .mount("/", routes![index])
}
