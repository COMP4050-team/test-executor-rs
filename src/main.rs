#[macro_use]
extern crate rocket;

use aws_sdk_s3::{types::ByteStream, Client, Region};
use rocket::{
    futures::{FutureExt, TryStreamExt},
    serde::{
        json::{serde_json, Json},
        Deserialize, Serialize,
    },
    State,
};
use std::{collections::HashSet, fs::File, io::Write};
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
    test: String,
    #[serde(rename = "SID")]
    sid: String,
    #[serde(rename = "Name")]
    name: String,
}

async fn download_file(
    client: &Client,
    bucket: &str,
    s3_key: &str,
    out_file: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Download test file
    let resp = client.get_object().bucket(bucket).key(s3_key).send().await;

    if let Err(e) = resp {
        println!("Error: {:?}", e);
        return Err("Error getting file from S3".into());
    }

    let stream = &mut resp.unwrap().body;

    let path = std::path::Path::new(out_file);
    let prefix = path.parent().unwrap();
    std::fs::create_dir_all(prefix).unwrap();

    let mut file = File::create(out_file).unwrap();
    while let Some(bytes) = stream.try_next().await.unwrap() {
        file.write(&bytes).unwrap();
    }

    Ok(())
}

async fn list_files<'a>(
    client: &'a Client,
    bucket: &str,
    prefix: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let resp = client
        .list_objects_v2()
        .bucket(bucket)
        .prefix(prefix)
        .send()
        .await;

    if let Err(e) = resp {
        println!("Error: {:?}", e);
        return Err("Error listing files from S3".into());
    }

    let files = resp
        .unwrap()
        .contents
        .unwrap()
        .iter_mut()
        .map(|f| f.key.as_ref().unwrap().to_owned())
        .collect();

    Ok(files)
}

#[post("/", data = "<task>")]
async fn index(
    cfg: &State<AppConfig>,
    client: &State<Client>,
    task: Json<Task<'_>>,
) -> &'static str {
    // Download test file
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

    // Create a map of project name to boolean
    // let mut result: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    let mut result: TestResult = TestResult { rows: vec![] };

    // Run processing-java on each of the project paths
    let tasks = project_paths
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

            spawn_blocking(move || {
                let output = std::process::Command::new("processing-java")
                    .arg("--force")
                    .arg(format!("--sketch=/tmp/projects/{path}"))
                    .arg(format!("--output=/tmp/output/{path}"))
                    .arg("--build")
                    .output()
                    .expect("failed to execute process");

                let student_details: Vec<&str> = student_info.split("_").collect();

                Row {
                    test: if output.status.success() {
                        "Passed".to_owned()
                    } else {
                        "Failed".to_owned()
                    },
                    sid: student_details[0].to_owned(),
                    name: format!(
                        "{} {}",
                        student_details[1].to_owned(),
                        student_details[2].to_owned()
                    ),
                }
            })
        })
        .collect::<Vec<_>>();

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
