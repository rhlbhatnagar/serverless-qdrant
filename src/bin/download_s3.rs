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

// async fn download_file(client: &s3::Client, bucket: &str, key: &str, dest: &str) -> Result<(), s3::Error> {
//     let resp = client.get_object().bucket(bucket).key(format!("storage/{}", key)).send().await?;

//     let body = resp.body.collect().await.unwrap();

//     // Create the directory if it doesn't exist
//     let parent_dir = Path::new(dest).parent().unwrap();
//     fs::create_dir_all(parent_dir).await.unwrap();

//     let mut file = fs::File::create(dest).await.unwrap();
//     file.write_all(&body.into_bytes()).await.unwrap();

//     Ok(())
// }

async fn download_file(
    client: &s3::Client,
    bucket: &str,
    key: &str,
    dest: &str,
) -> Result<(), s3::Error> {
    let resp = client
        .get_object()
        .bucket(bucket)
        .key(format!("storage/{}", key))
        .send()
        .await?;
    warn!("{}", dest);
    let mut file = fs::File::create(dest).await.unwrap();
    let mut body = resp.body;
    while let Some(chunk) = body.next().await {
        let chunk = chunk.unwrap();
        file.write_all(&chunk).await.unwrap();
    }
    Ok(())
}

#[async_recursion]
async fn process_dir(
    client: Arc<s3::Client>,
    bucket: String,
    path: String,
    prefix: String,
    counter: Arc<Mutex<u32>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut tasks = vec![];
    let mut dir_entries = fs::read_dir(&path).await?;

    while let Some(entry) = dir_entries.next_entry().await? {
        let path = entry.path();
        let key = format!("{}{}", prefix, entry.file_name().to_str().unwrap());

        if path.is_dir() {
            let client_clone = Arc::clone(&client);
            let bucket_clone = bucket.clone();
            let new_prefix = format!("{}/", key);
            let counter_clone = Arc::clone(&counter);
            let path_str = path.to_str().unwrap().to_owned();
            tasks.push(tokio::spawn(async move {
                // Box the future here to avoid the cycle
                let process_future = process_dir(
                    client_clone,
                    bucket_clone,
                    path_str,
                    new_prefix,
                    counter_clone,
                );
                let boxed_future = Box::pin(process_future);
                boxed_future.await
            }));
        } else {
            let client_clone = Arc::clone(&client);
            let bucket_clone = bucket.clone();
            let dest = format!("/tmp/{}", key);
            let counter_clone = Arc::clone(&counter);
            tasks.push(tokio::spawn(async move {
                let result = download_file(&client_clone, &bucket_clone, &key, &dest).await;
                if result.is_ok() {
                    let mut num_files = counter_clone.lock().await;
                    *num_files += 1;
                }
                result.map_err(|e| e.into())
            }));
        }
    }

    let results = join_all(tasks).await;
    for result in results {
        result??;
    }

    // for task in tasks {
    //     task.await??;
    // }

    Ok(())
}

#[derive(Deserialize, Serialize)]
pub struct EmailSendRequest {
    pub bucket: Option<String>,
    pub path: Option<String>,
}

async fn lambda_handler(event: EmailSendRequest, _: Context) -> Result<(), Error> {
    let bucket = "qdrantlambdastack-s3bucket07682993-hsduqsiqbibh"; //env::var("BUCKET_NAME").expect("BUCKET_NAME must be set");
    let dest = "/tmp/".to_string(); // env::var("PATH").expect("PATH must be set");

    let shared_config = aws_config::load_from_env().await;
    let client = Arc::new(s3::Client::new(&shared_config));

    let start_time = Instant::now();
    download_s3_objects(client, bucket, "storage", "/tmp").await;
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

    for object in resp.contents.unwrap_or_default() {
        let key = object.key.unwrap();
        let client_clone = Arc::clone(&client);
        let bucket_clone = bucket.to_string();
        let dest_clone = dest.to_string();
        tasks.push(tokio::spawn(async move {
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
