use std::{
    borrow::Cow,
    io::{BufRead, Seek},
    path::Path,
    sync::Arc,
};

use anyhow::{Context, anyhow};
use cushy::{
    animation::ZeroToOne,
    figures::Zero,
    kludgine::{AnyTexture, LazyTexture, image::ImageReader},
    reactive::value::{Destination, Dynamic, Source},
    widget::MakeWidget,
    widgets::{label::Displayable, progress::Progressable},
};
use directories::ProjectDirs;
use rc_zip_tokio::ReadZip;
use reqwest::Client;
use tokio::io::{AsyncRead, AsyncSeek, AsyncSeekExt, AsyncWriteExt};

#[derive(Default, Clone, Debug, PartialEq)]
pub struct InProgress {
    pub percent: Dynamic<ZeroToOne>,
    pub name: String,
}

impl MakeWidget for InProgress {
    fn make_widget(self) -> cushy::widget::WidgetInstance {
        self.percent
            .clone()
            .map_each(|percent| format!("{:.2}%", **percent * 100.0))
            .to_label()
            .and(
                self.percent
                    .progress_bar()
                    .pad()
                    .expand_horizontally()
                    .centered(),
            )
            .into_columns()
            .make_widget()
    }
}

#[derive(Clone, Debug)]
pub struct WiiResources {
    directories: ProjectDirs,
    client: Client,
    pub downloads: Dynamic<Vec<InProgress>>,
}

type R<T = ()> = anyhow::Result<T>;

impl WiiResources {
    pub fn new(directories: ProjectDirs) -> Self {
        Self {
            directories,
            client: Client::builder()
                .build()
                .expect("Failed to init http client"),
            downloads: Dynamic::default(),
        }
    }

    async fn cache_dir(&self) -> R<&Path> {
        let cache = self.directories.cache_dir();
        tokio::fs::create_dir_all(cache).await?;
        Ok(cache)
    }

    pub async fn download(&self) -> R {
        self.download_url(
            "https://www.gametdb.com/download.php?FTP=GameTDB-wii_disc-US-2025-03-19.zip",
            "discs",
        )
        .await?;
        Ok(())
    }

    async fn download_url(&self, download_url: &str, name: &str) -> R {
        let download_url =
            "https://www.gametdb.com/download.php?FTP=GameTDB-wii_disc-US-2025-03-19.zip";
        println!("Requesting file");
        let mut response = self.client.get(download_url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("Failed to download covers"));
        }
        let file_size = response.content_length();
        let mut progress = file_size.map(|file_size| {
            let progress = InProgress {
                name: name.to_string(),
                ..Default::default()
            };
            self.downloads
                .map_mut(|mut downloads| downloads.push(progress.clone()));
            (progress, file_size)
        });
        let zip_path = self.cache_dir().await?.join(format!("{name}.zip"));
        let mut file = tokio::fs::File::create(&zip_path)
            .await
            .map(tokio::io::BufWriter::new)?;

        let mut consumed = 0usize;
        println!("Downloading file");
        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;
            if let Some((progress, file_size)) = progress.as_mut() {
                consumed += chunk.len();
                progress
                    .percent
                    .set((consumed as f32 / *file_size as f32).into());
            }
        }
        file.flush().await.unwrap();
        std::mem::drop(file);
        println!("Reopening file");
        let file = Arc::new(positioned_io::RandomAccessFile::open(zip_path)?);
        println!("Reading as zip");
        let zip = file.read_zip().await?;
        let cache = self.cache_dir().await?;
        for entry in zip.entries() {
            let path = cache.join(&entry.name);
            println!("Extracting: {path:?}");
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(path, entry.bytes().await?).await?;
        }

        Ok(())
    }
    const DEFAULT_IMAGE: &[u8] = include_bytes!("../assets/default-cover.jpg");
    fn load_image<'a>(img: impl BufRead + Seek + 'a) -> AnyTexture {
        let image = ImageReader::new(img)
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap();
        LazyTexture::from_image(image, cushy::kludgine::wgpu::FilterMode::Nearest).into()
    }

    pub async fn get_cover(&self, id: &str) -> R<AnyTexture> {
        let coverdir = self
            .cache_dir()
            .await?
            .join(format!("wii/disc/US/{id}.png"));
        let bytes = tokio::fs::read(coverdir)
            .await
            .map(Cow::Owned)
            .unwrap_or(Cow::Borrowed(Self::DEFAULT_IMAGE));

        Ok(Self::load_image(std::io::Cursor::new(bytes)))
    }
}
