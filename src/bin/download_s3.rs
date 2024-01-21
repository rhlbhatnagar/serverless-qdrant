use std::env;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use async_recursion::async_recursion;
use aws_lambda_events::event::s3::S3Event;
use aws_sdk_s3 as s3;
use futures::future::join_all;
use lambda_runtime::{handler_fn, Context, Error, LambdaEvent};
use log::warn;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::sync::Semaphore;


async fn download_file(client: &s3::Client, bucket: &str, key: &str, dest: &str) -> Result<(), s3::Error> {
    let resp = client.get_object().bucket(bucket).key(format!("storage/{}", key)).send().await?;

    let body = resp.body.collect().await.unwrap();

    // Create the directory if it doesn't exist
    let parent_dir = Path::new(dest).parent().unwrap();
    fs::create_dir_all(parent_dir).await.unwrap();

    let mut file = fs::File::create(dest).await.unwrap();
    file.write_all(&body.into_bytes()).await.unwrap();

    Ok(())
}

#[derive(Deserialize, Serialize)]
pub struct DownloadFileReq {
    pub bucket: Option<String>,
    pub path: Option<String>,
}


async fn lambda_handler(event: DownloadFileReq, _: Context) -> Result<(), Error> {
    let bucket = "qdrantlambdastack-s3bucket07682993-hsduqsiqbibh"; //env::var("BUCKET_NAME").expect("BUCKET_NAME must be set");
    let dest = "/mnt/efs"; // env::var("PATH").expect("PATH must be set");

    let shared_config = aws_config::load_from_env().await;
    let client = Arc::new(s3::Client::new(&shared_config));

    let start_time = Instant::now();
    download_s3_objects(client, bucket, "storage", dest).await;
    //    process_dir(client, bucket, path, "".to_string(), counter.clone()).await.unwrap();
    let duration = start_time.elapsed();

    println!("Copied files in {:?}", duration);

    Ok(())
}

async fn download_s3_objects(
    client: Arc<s3::Client>,
    bucket: &str,
    path: &str,
    dest: &str,
) -> Result<(), Error> {
    let resp = client
        .list_objects_v2()
        .bucket(bucket)
        .prefix(path)
        .send()
        .await?;

    let mut tasks = vec![];

    let semaphore = Arc::new(Semaphore::new(5)); // Adjust this number based on your memory constraints


    for object in resp.contents.unwrap_or_default() {
        let key = object.key.unwrap();
        let client_clone = Arc::clone(&client);
        let bucket_clone = bucket.to_string();
        let dest_clone = dest.to_string();
        let semaphore_clone = Arc::clone(&semaphore); // Clone the semaphore here
        tasks.push(tokio::spawn(async move {
            // Acquire a permit from the semaphore before starting the download
            let _permit = semaphore_clone.acquire().await;
            let resp = client_clone
                .get_object()
                .bucket(&bucket_clone)
                .key(&key)
                .send()
                .await?;
            let body = resp.body.collect().await.unwrap();
            let dest_path = format!("{}/{}", dest_clone, key);
            let parent_dir = std::path::Path::new(&dest_path).parent().unwrap();
            fs::create_dir_all(parent_dir).await.unwrap();
            let mut file = fs::File::create(&dest_path).await.unwrap();
            file.write_all(&body.into_bytes()).await.unwrap();
            Ok::<(), Error>(())
        }));
    }

    let results = join_all(tasks).await;
    for result in results {
        result??;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    //lambda_handler().await;
    lambda_runtime::run(handler_fn(lambda_handler))
        .await
        .unwrap();
    Ok(())
}
