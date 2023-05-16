use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use thirtyfour::prelude::*;
use tokio::sync::mpsc::{UnboundedReceiver};
use url::Url;

#[derive(Debug)]
pub struct Messages {
    images_url: Vec<String>,
    pub save_path: PathBuf,
}
#[derive(Debug)]
pub struct Message {
    pub image_url: String,
    pub save_path: PathBuf,
}

impl Messages {
    pub fn flatten_message(self) -> Vec<Message> {
        let mut vec = Vec::with_capacity(self.images_url.capacity());
        let mut n = 1_usize;
        for image_url in self.images_url {
            vec.push(Message {
                image_url,
                save_path: self.save_path.join(format!("{n:05}.png")),
            });
            n += 1;
        }
        vec
    }
}

#[derive(Debug)]
pub struct ChapterInfo {
    // pub images_url: Vec<String>,
    pub chapter_name: String,
    pub chapter_url: String,
}

pub struct ComicDriver(WebDriver);

impl ComicDriver {
    pub fn new(driver: WebDriver) -> Self {
        Self(driver)
    }

    pub async fn process_page(self, url: &str) -> anyhow::Result<ComicInfo> {
        let driver = &self.0;
        driver.goto(url).await?;
        let mut url = Url::parse(url)?;

        let name_element = driver
            .find(By::ClassName("comicParticulars-title-right"))
            .await?;
        let name_element = name_element.query(By::Tag("h6")).first().await?;
        let name = name_element.text().await?;

        let mut items = vec![];
        let url_list_element = driver.query(By::Css("div[id^=default]")).first().await?;
        let ul_element = url_list_element.query(By::Tag("ul")).first().await?;
        let url_list = ul_element.query(By::Tag("a")).all().await?;

        for a in url_list {
            let href = a.attr("href").await?.unwrap();
            url.set_path(&href);
            let name = a.attr("title").await?.unwrap();
            // let name_element = a.query(By::Tag("li")).first().await?;
            items.push(ChapterInfo {
                chapter_name: name,
                chapter_url: url.to_string(),
            });
        }

        Ok(ComicInfo {
            comic_name: name,
            items,
            driver: self.0,
        })
    }
}

pub struct ComicInfo {
    pub comic_name: String,
    pub items: Vec<ChapterInfo>,
    driver: WebDriver,
}

impl ComicInfo {
    pub async fn process_page(
        self,
        save_path: impl AsRef<Path>,
    ) -> anyhow::Result<UnboundedReceiver<Messages>> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let save_path = save_path.as_ref().to_owned();

        tokio::spawn(async move {
            let driver = self.driver;

            for info in &self.items {
                driver.goto(&info.chapter_url).await?;
                // let name_element = driver.find(By::ClassName("header")).await?;
                // let name = name_element.text().await?;
                // let name = name.split_once('/').unwrap().1.to_string();
                // let name = name.trim_start_matches('/').to_string();
                let count = Self::page_prepare(&driver).await? as usize;
                let images_url = Self::get_image_list(&driver).await?;

                anyhow::ensure!(count == images_url.len(), "consist error");
                let save_path = save_path.join(&self.comic_name).join(&info.chapter_name);
                // println!("{count} {}", images_url.len());
                tx.send(Messages {
                    save_path,
                    images_url,
                })?;
            }
            driver.quit().await?;
            Ok(())
        });
        Ok(rx)
    }

    async fn page_prepare(driver: &WebDriver) -> anyhow::Result<u32> {
        const SPEED: f32 = 0.1;

        let page_index_element = driver.find(By::ClassName("comicIndex")).await?;
        let page_count_element = driver.find(By::ClassName("comicCount")).await?;
        let page_count: u32 = page_count_element.text().await?.parse().unwrap();
        let mut index = 0;
        let key = Key::PageDown.to_string();
        tokio::time::sleep(Duration::from_secs_f32(2.)).await;
        while index != page_count {
            tokio::time::sleep(Duration::from_secs_f32(SPEED)).await;
            driver.action_chain().send_keys(&key).perform().await?;
            index = page_index_element.text().await?.parse().unwrap();
            tokio::time::sleep(Duration::from_secs_f32(SPEED)).await;
            driver.action_chain().send_keys(&key).perform().await?;
        }
        Ok(page_count)
    }

    async fn get_image_list(driver: &WebDriver) -> anyhow::Result<Vec<String>> {
        let ul = driver.find(By::ClassName("comicContent-list")).await?;
        let image_list = ul.query(By::Tag("img")).all().await?;
        let mut ret = vec![];
        for image in image_list {
            ret.push(image.attr("data-src").await?.unwrap());
        }
        Ok(ret)
    }
}
