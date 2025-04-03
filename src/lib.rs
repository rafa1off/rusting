use std::{env, time::Duration};

use async_channel::{Receiver, Sender};
use reqwest::Client;
use tokio::{
    fs,
    task::JoinHandle,
    time::{Instant, timeout},
};

fn read_args() -> (String, usize) {
    let mut args = env::args();
    args.next();

    let path = args
        .next()
        .expect("Usage: 'rusting <path/to/csv> [OPTIONAL] <workers>'");

    let workers = match args.next() {
        Some(x) => x.parse().unwrap(),
        None => num_cpus::get(),
    };

    (path, workers)
}

async fn request_pool(channel: Receiver<String>, workers: usize) -> Vec<JoinHandle<()>> {
    let client = Client::new();
    let mut handlers = Vec::with_capacity(workers);

    for i in 0..workers {
        let rt = Receiver::clone(&channel);
        let cl = Client::clone(&client);
        handlers.push(tokio::spawn(async move {
            while let Ok(url) = rt.recv().await {
                let start = Instant::now();
                let timeout_duration = Duration::from_secs(30);

                let request_fut = cl.get(&url).send();
                let timeout_fut = timeout(timeout_duration, request_fut);

                match timeout_fut.await {
                    Ok(request) => match request {
                        Ok(response) => println!(
                            "[Worker {i}]: {0} -> {1}: in {2:.2}s",
                            response.url(),
                            response.status(),
                            (Instant::now() - start).as_secs_f32()
                        ),
                        Err(_) => {
                            println!(
                                "[Worker {i}]: {0} -> 404 Not found: in {1:.2}s",
                                url,
                                (Instant::now() - start).as_secs_f32()
                            );
                        }
                    },
                    Err(_) => {
                        println!(
                            "[Worker {i}]: {0} -> Timeout: in {1:.2}s",
                            url,
                            (Instant::now() - start).as_secs_f32()
                        );
                    }
                };
            }
        }));
    }

    handlers
}

async fn stream_url(channel: Sender<String>, data: String) {
    for raw_url in data.split(',') {
        let url = String::from(raw_url.trim()).replace("\n", "");

        channel.send(url).await.unwrap();
    }
}

pub async fn read_csv() {
    let (path, workers) = read_args();

    let data = fs::read_to_string(path)
        .await
        .expect("Error reading file");

    let (sx, rx) = async_channel::unbounded();

    let mut pool = request_pool(rx, workers).await;

    stream_url(sx, data).await;

    for worker in pool.drain(..) {
        worker.await.unwrap();
    }
}
