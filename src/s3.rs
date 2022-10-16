use std::{fs::File, io::Write};

use aws_sdk_s3::Client;
use rocket::futures::TryStreamExt;

pub async fn download_file(
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

pub async fn list_files<'a>(
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
