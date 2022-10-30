mod junit_xml;
mod s3;

#[macro_use]
extern crate rocket;

use crate::junit_xml::{TestSuite, TestSuiteChild};
use aws_sdk_s3::{types::ByteStream, Client, Region};
use rocket::tokio::spawn;
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
    fs::File,
    io::{Read, Write},
};
use tempfile::tempdir;
use tokio::task::spawn_blocking;

struct AppConfig {
    bucket_name: String,
}

#[derive(Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
struct Task {
    // Deserialize the following field as `s3KeyTestFile` instead of `s3_key_test_file`.
    #[serde(rename = "s3KeyTestFile")]
    s3_key_test_file: String,
    #[serde(rename = "s3KeyProjectFile")]
    s3_key_project_file: String,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct TestResult {
    results: Vec<StudentResult>,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct StudentResult {
    #[serde(rename = "tests")]
    tests: Vec<UnitTestResult>,
    #[serde(rename = "student_id")]
    student_id: String,
    #[serde(rename = "student_name")]
    student_name: String,
}

#[derive(Serialize)]
#[serde(crate = "rocket::serde")]
struct UnitTestResult {
    #[serde(rename = "name")]
    name: String,
    #[serde(rename = "passed")]
    passed: bool,
    #[serde(rename = "message")]
    message: String,
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
    let output = std::process::Command::new("./gradlew")
        .arg("test")
        .current_dir(project_path)
        .output()
        .expect("failed to execute process");

    println!("status: {}", output.status);
    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
}

#[post("/", data = "<task>")]
fn index(cfg: &State<AppConfig>, task: Json<Task>) -> &'static str {
    // Make a hard copy of cfg.bucket_name
    let bucket_name = cfg.bucket_name.clone();

    spawn(run_tests(bucket_name, task));

    "Running!"
}

async fn run_tests(bucket_name: String, task: Json<Task>) {
    let aws_config = aws_config::from_env()
        .region(Region::new("ap-southeast-2"))
        .load()
        .now_or_never()
        .unwrap();
    let client = Client::new(&aws_config);

    let test_run_temp_dir = tempdir().unwrap().into_path();
    // std::fs::create_dir_all(&test_run_temp_dir).unwrap();

    let test_file_path = test_run_temp_dir.join("Test.java");
    let project_directory = test_run_temp_dir.join("project");
    let output_directory = test_run_temp_dir.join("output");

    // Download specified test file
    download_file(
        &client,
        &bucket_name,
        &task.s3_key_test_file,
        test_file_path.to_str().unwrap(),
    )
    .await
    .unwrap();

    // List all the files in the project
    let files = list_files(&client, &bucket_name, &task.s3_key_project_file)
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
            &bucket_name,
            file,
            project_directory.join(file).to_str().unwrap(),
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

    let mut result: TestResult = TestResult { results: vec![] };
    let mut tasks = vec![];

    // Run processing-java on each of the project paths as well as the tests using gradle
    for project_path in &project_paths {
        let project_path = project_path.to_owned();
        let student_info = std::path::Path::new(&project_path)
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let project_name = std::path::Path::new(&project_path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();

        let project_directory = project_directory.clone();
        let output_directory = output_directory.clone();
        let test_file_path = test_file_path.clone();
        let handle = spawn_blocking(move || {
            let student_details: Vec<&str> = student_info.split('_').collect();
            let sid = student_details[0];
            let first_name = student_details[1];
            let last_name = student_details[2];

            let project_temp_dir = tempdir().unwrap().into_path().join(sid);
            // std::fs::create_dir_all(&project_temp_dir).unwrap();

            // Run processing-java
            let output = std::process::Command::new("processing-java")
                .arg("--force")
                .arg(format!(
                    "--sketch={}",
                    project_directory.join(&project_path).to_str().unwrap()
                ))
                .arg(format!(
                    "--output={}",
                    &output_directory.join(&project_path).to_str().unwrap()
                ))
                .arg("--build")
                .output()
                .expect("failed to execute process");

            if !output.status.success() {
                println!("Error: {}", String::from_utf8_lossy(&output.stderr));
            }

            // Copy the java project template for this project
            println!(
                "Running: cp -r templates/testing-project/ {}",
                project_temp_dir.to_str().unwrap()
            );
            let output = std::process::Command::new("cp")
                .arg("-r")
                .arg("templates/testing-project/")
                .arg(&project_temp_dir)
                .output()
                .expect("failed to copy testing project");

            if !output.status.success() {
                println!("Error: {}", String::from_utf8_lossy(&output.stderr));
            }

            println!(
                "Copying {} to {}/src/main/java/org/example/Test.java",
                test_file_path.to_str().unwrap(),
                project_temp_dir.to_str().unwrap()
            );

            // Move the downloaded test file at `test_file_path` to {temp_dir}/src/test/java/org/example/Test.java
            std::fs::copy(
                test_file_path.to_str().unwrap(),
                project_temp_dir.join("src/test/java/org/example/Test.java"),
            )
            .unwrap();

            println!(
                    "Copying {}/source/{project_name}.java to {}/src/main/java/org/example/{project_name}.java", &output_directory.join(&project_path).to_str().unwrap(), project_temp_dir.to_str().unwrap()
                );

            // Move the compiled {project_name}.java to {temp_dir}/src/main/java/org/example/{project_name}.java
            std::fs::copy(
                format!(
                    "{}/source/{project_name}.java",
                    output_directory.join(&project_path).to_str().unwrap()
                ),
                project_temp_dir.join(format!("src/main/java/org/example/{project_name}.java")),
            )
            .unwrap();

            // Add the org.example package to the file
            prepend_to_file(
                project_temp_dir
                    .join(format!("src/main/java/org/example/{project_name}.java"))
                    .to_str()
                    .unwrap(),
                "package org.example;",
            )
            .unwrap();

            // Run the tests with gradle
            run_tests_with_gradle(project_temp_dir.to_str().unwrap());

            // Parse the test result xml file
            let mut f = File::open(
                project_temp_dir.join("build/test-results/test/TEST-org.example.TestProject.xml"),
            )
            .unwrap();
            let mut contents = String::new();
            f.read_to_string(&mut contents).unwrap();
            let test_suite = from_str::<TestSuite>(&contents).unwrap();

            // Delete the temp directory
            // std::fs::remove_dir_all(&project_temp_dir).unwrap();

            let mut unit_test_results = vec![];

            test_suite.children.iter().for_each(|test_suite_child| {
                if let TestSuiteChild::TestCase(testcase) = test_suite_child {
                    // set message to the failures message if there is a failure or the errors message if there is an error
                    let message = match &testcase.failure {
                        Some(failure) => &failure.message,
                        None => match &testcase.error {
                            Some(error) => error,
                            None => "",
                        },
                    };

                    unit_test_results.push(UnitTestResult {
                        name: testcase.name.clone(),
                        passed: testcase.failure == None && testcase.error == None,
                        message: message.to_string(),
                    });
                }
            });

            StudentResult {
                tests: unit_test_results,
                student_id: sid.to_owned(),
                student_name: format!("{} {}", first_name.to_owned(), last_name.to_owned()),
            }
        });

        tasks.push(handle);
    }

    for thread in tasks {
        result.results.push(thread.await.unwrap());
    }

    let serialised = serde_json::to_string(&result).unwrap();
    let body = ByteStream::from(serialised.as_bytes().to_vec());

    // Get S3 assignment directory
    let assignment_dir = std::path::Path::new(&task.s3_key_project_file)
        .parent()
        .unwrap()
        .to_str()
        .unwrap();

    // Upload compile_error as a json file to S3
    let resp = client
        .put_object()
        .bucket(bucket_name)
        .key(format!("{}/Results/result.json", assignment_dir))
        .body(body)
        .send()
        .await;

    if let Err(e) = resp {
        println!("Error: {:?}", e);
    }
}

#[launch]
fn rocket() -> _ {
    let cfg = AppConfig {
        bucket_name: "uploads-76078f4".into(),
    };

    rocket::build().manage(cfg).mount("/", routes![index])
}
