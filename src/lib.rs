use std::{env, time::Duration};

use async_channel::{Receiver, Sender};
use tokio::{
    fs,
    task::JoinHandle,
    time::{Instant, timeout},
};

async fn read_args() -> (String, i32) {
    let mut args = env::args();
    args.next();

    let path = args
        .next()
        .expect("Usage: 'rusting <path/to/csv> [OPTIONAL] <workers>'");

    let workers = match args.next() {
        Some(x) => x.parse::<i32>().unwrap(),
        None => num_cpus::get().try_into().unwrap(),
    };

    (path, workers)
}

async fn request_pool(channel: Receiver<String>, handlers: &mut Vec<JoinHandle<()>>, workers: i32) {
    for i in 0..workers {
        let rt = channel.clone();
        handlers.push(tokio::spawn(async move {
            while let Ok(url) = rt.recv().await {
                let start = Instant::now();
                match timeout(Duration::from_secs(30), reqwest::get(&url)).await {
                    Ok(pass) => match pass {
                        Ok(res) => println!(
                            "[Worker {i}]: {0} -> {1}: in {2:.2}s",
                            res.url(),
                            res.status(),
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
}

async fn stream_url(channel: Sender<String>, data: String) {
    for url in data.split(',') {
        channel
            .send(String::from(url.trim()).replace("\n", ""))
            .await
            .unwrap();
    }
}

pub async fn read_csv() {
    let (path, workers) = read_args().await;

    let data = fs::read_to_string(path);

    let (sx, rx) = async_channel::unbounded();

    let mut handlers = Vec::new();

    let pool_fut = request_pool(rx, &mut handlers, workers);

    let stream_fut = stream_url(
        sx,
        data.await
            .expect("Error reading file path"),
    );

    pool_fut.await;
    stream_fut.await;
    for handler in handlers.drain(..) {
        handler.await.unwrap();
    }
}
