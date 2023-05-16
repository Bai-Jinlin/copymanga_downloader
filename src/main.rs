use bytes::Bytes;
use comic::{Message, Messages};
use settings::Settings;
use if_chain::if_chain;
use image::io::Reader as ImageReader;
use image::{EncodableLayout, ImageOutputFormat};
use state::Storage;

use std::io::Cursor;
use std::path::PathBuf;
use std::time::Duration;
use threadpool::ThreadPool;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::mpsc::{Receiver, UnboundedReceiver};

use thirtyfour::{prelude::*, CapabilitiesHelper};

use crate::comic::ComicDriver;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod comic;
mod settings;
mod driver_helper;
mod utils;

static SETTINGS: Storage<Settings> = Storage::new();

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    SETTINGS.set(Settings::new()?);

    let settings = SETTINGS.get();

    let tx = driver_helper::start_firefox_driver(
        &SETTINGS.get().driver.driver_path,
        &SETTINGS.get().driver.firefox_binary_path,
    );

    let mut caps = DesiredCapabilities::firefox();

    if let Some(proxy) = settings.http_proxy.as_deref() {
        let proxy = proxy.strip_prefix("http://").unwrap().to_owned();
        tracing::debug!("proxy set: {}",proxy);

        let proxy_config = thirtyfour::Proxy::Manual {
            http_proxy: Some(proxy.clone()),
            ssl_proxy: Some(proxy),
            socks_proxy: None,
            socks_version: None,
            socks_username: None,
            socks_password: None,
            ftp_proxy: None,
            no_proxy: None,
        };
        caps.set_proxy(proxy_config)?;
    }

    let driver = WebDriver::new("http://localhost:4444", caps).await?;
    let comic_driver = ComicDriver::new(driver);
    run(comic_driver).await?;

    let _ = tx.send(());
    Ok(())
}

async fn run(comic: ComicDriver) -> anyhow::Result<()> {
    let info = comic
        .process_page("{}")
        .await?;

    // info.items = info.items.into_iter().skip(2).take(1).collect();
    let rx = info.process_page(".").await?;
    process_msgs(rx).await?;

    Ok(())
}

async fn process_msgs(mut msgs_rx: UnboundedReceiver<Messages>) -> anyhow::Result<()> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    let handle = tokio::spawn(download_images(rx));

    while let Some(msgs) = msgs_rx.recv().await {
        tokio::fs::create_dir_all(&msgs.save_path).await?;
        let msgs = msgs.flatten_message();
        for msg in msgs {
            tx.send(msg)?;
        }
    }
    drop(tx);
    let _ = handle.await;
    Ok(())
}

async fn download_images(mut msg_rx: UnboundedReceiver<Message>) -> anyhow::Result<()> {
    // let semaphore = Semaphore::new(10);
    let mut client_builder = reqwest::Client::builder();
    let settings = SETTINGS.get();
    if let Some(proxy) = settings.http_proxy.as_deref() {
        let proxy_config = reqwest::Proxy::all(proxy).unwrap();
        client_builder=client_builder.proxy(proxy_config);
    }
    let client = client_builder.build()?;

    let (tx, rx) = tokio::sync::mpsc::channel(30);
    let handle = tokio::task::spawn_blocking(move || process_image(rx));

    while let Some(msg) = msg_rx.recv().await {
        {
            let tx = tx.clone();
            let client = client.clone();
            tokio::spawn(async move {
                if_chain! {
                    if let Ok(rep)=client.get(&msg.image_url).send().await;
                    if let Ok(bytes) = rep.bytes().await;
                    then {
                        let _=tx.send((bytes,msg.save_path)).await;

                    } else{
                        tracing::error!("image download error,url: {},save_path: {}",msg.image_url,msg.save_path.display());
                    }
                }
            });
        }
    }
    drop(tx);
    handle.await?;
    Ok(())
}

fn process_image(mut rx: Receiver<(Bytes, PathBuf)>) {
    let pool = ThreadPool::new(8);
    'out: loop {
        match rx.try_recv() {
            Ok((bytes, path)) => pool.execute(move || {
                let mut file = std::fs::File::create(path).unwrap();
                let img = ImageReader::new(Cursor::new(bytes.as_bytes()))
                    .with_guessed_format()
                    .unwrap()
                    .decode()
                    .unwrap();
                img.write_to(&mut file, ImageOutputFormat::Png).unwrap();
            }),
            Err(TryRecvError::Disconnected) => {
                break 'out;
            }
            _ => {
                std::thread::sleep(Duration::from_secs_f32(0.5));
            }
        }
    }

    pool.join();
}

// async fn run(comic: ComicDriver<'_>) -> anyhow::Result<()> {
//     let info = comic
//         .process_info_page("https://www.copymanga.site/comic/zhuanshengwangnvhetiancaiqianjindemofageming")
//         .await?;
//     let ComicIndexInfo { comic_name: name, items } = info;
//     let out_path = Path::new(&name);
//     tokio::fs::create_dir_all(out_path).await?;

//     for item in items.into_iter(){
//         let (bytes_vec, path) = download_comic(&comic, item, out_path).await?;

//         let handle = tokio::task::spawn_blocking(move || {
//             bytes_vec.into_par_iter().for_each(|(bytes, name)| {
//                 let mut file = std::fs::File::create(path.join(name)).unwrap();
//                 let img = ImageReader::new(Cursor::new(
//                     bytes.as_bytes()
//                 )).with_guessed_format().unwrap().decode().unwrap();
//                 img.write_to(&mut file, ImageOutputFormat::Png).unwrap();
//             });
//         });
//         handle.await?;
//     }
//     Ok(())
// }

// async fn download_comic(
//     comic: &ComicDriver<'_>,
//     item: ComicIndexItem,
//     path: impl AsRef<Path>,
// ) -> anyhow::Result<(Vec<(Bytes, String)>, PathBuf)> {
//     let page = comic.process_comic_page(&item.index_url).await?;
//     let out_path = path.as_ref().join(&page.comic_name);
//     tokio::fs::create_dir_all(&out_path).await?;

//     let mut handles = vec![];

//     for url in page.images_url {
//         handles.push(
//             tokio::spawn(async move {
//                 let response = reqwest::get(url).await?;
//                 let bytes = response.bytes().await?;
//                 Ok::<_, anyhow::Error>(bytes)
//             })
//         );
//     }

//     let mut ret = vec![];
//     let mut n = 1;

//     for handle in handles {
//         let bytes = handle.await??;
//         ret.push((bytes, format!("{n:05}.png")));
//         n += 1;
//     }
//     Ok((ret, out_path))
// }
