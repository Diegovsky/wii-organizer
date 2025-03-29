use anyhow::Context;
use covers::WiiResources;
use cushy::reactive::value::{Destination, Dynamic};
use cushy::styles::VerticalAlign;
use cushy::widgets::VirtualList;
use cushy::{Application, Run};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::mpsc::{Receiver, Sender};

use cushy::figures::Size;
use cushy::{
    App, MaybeLocalized, Open,
    dialog::{FilePicker, PickFile},
    figures::{Zero, units::Lp},
    kludgine::AnyTexture,
    reactive::value::{Source, Watcher},
    styles::{
        CornerRadii, Dimension,
        components::{CornerRadius, VerticalAlignment},
    },
    widget::MakeWidget,
    widgets::{Image, input::InputValue, label::Displayable, layers::OverlayLayer},
    window::{PendingWindow, WindowHandle},
};

mod covers;
mod game;
mod utils;
//mod menu;
use game::Game;
pub use utils::*;

type R<T = ()> = anyhow::Result<T>;

fn menu(window_overlay: OverlayLayer, data: AppData) -> impl MakeWidget {
    let _ = window_overlay;
    let wbfs_dir = data.wbfs_folder.clone();
    "Open WBFS Folder..."
        .to_button()
        .on_click(move |_c| {
            let wbfs_dir = wbfs_dir.clone();
            main_window_handle().pick_folder(
                &FilePicker::new()
                    .with_title("Pick your wbfs folder")
                    .with_initial_directory(std::env::current_dir().unwrap()),
                move |folder| {
                    if let Some(mut folder) = folder {
                        if !folder.ends_with("wbfs") {
                            folder = folder.join("wbfs");
                            std::fs::create_dir_all(&folder).unwrap();
                        }

                        wbfs_dir.set(folder)
                    }
                },
            );
        })
        .and("Download covers".to_button().on_click({let data = data.clone(); move |_| {
            data.spawn_with_clone(|data| async move { data.covers.download().await.unwrap() });
        }}))
        .and("funny button".to_button().on_click(closure!([data] (_) data.wbfs_folder.set("/tmp/idosas/wbfs".into()))))
        .and("Add WBFS game...".to_button().on_click(closure!([data] (_c) {
            main_window_handle().pick_file(
                &FilePicker::new()
                    .with_title("Select a WBFS game")
                    .with_initial_directory(std::env::current_dir().unwrap()),
                closure!([sender = data.sender.clone()] (file) {
                    if let Some(file) = file {
                        if file.extension().unwrap_or_default() != "wbfs" {
                            println!("Invalid file extension");
                            return
                        }
                        sender.update(Message::AddWbfs(file))
                    }
                }),
            );
        })))
        .into_columns()
        .gutter(Lp::new(2))
        .with_styles(
            cushy::styles!(CornerRadius => CornerRadii::<Dimension>::ZERO, VerticalAlignment => VerticalAlign::Center)
            )
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
struct CachedData {
    pub wbfs_folder: Dynamic<PathBuf>,
    #[serde(skip)]
    path: PathBuf,
}

impl CachedData {
    fn file(dirs: &ProjectDirs) -> PathBuf {
        dbg!(dirs.cache_dir().join("prefs.json"))
    }
    async fn save(&self) -> R<()> {
        let parent = self.path.parent().context("Invalid path")?;
        tokio::fs::create_dir_all(parent)
            .await
            .context("Failed to create prefs dir")?;
        let contents = serde_json::to_vec(&self)?;
        tokio::fs::write(&self.path, contents)
            .await
            .context("Failed to write to prefs file")?;
        Ok(())
    }
    fn _load(path: &PathBuf) -> R<Self> {
        let file = std::fs::File::open(path).map(std::io::BufReader::new)?;
        Ok(serde_json::from_reader(file)?)
    }
    fn load(dirs: &ProjectDirs) -> Self {
        let path = Self::file(dirs);
        let mut this = Self::_load(&path).unwrap_or_default();
        this.path = path;
        this
    }
}

#[derive(Clone)]
struct AppData {
    pub gamelist: Gamelist,
    pub wbfs_folder: Dynamic<PathBuf>,
    pub covers: WiiResources,
    pub sender: Sender<Message>,
}

impl AppData {
    fn new(
        window_title: Dynamic<MaybeLocalized>,
        dirs: &ProjectDirs,
        cached: &CachedData,
        sender: Sender<Message>,
    ) -> Self {
        let gamelist: Gamelist = Default::default();

        let wbfs_folder = cached.wbfs_folder.clone().with_for_each(move |path| {
            window_title.map_mut(|mut val| {
                if !path.as_os_str().is_empty() {
                    *val = format!("Wii Manager: {}", path.display()).into()
                }
            })
        });
        let this = Self {
            gamelist,
            wbfs_folder,
            sender,
            covers: WiiResources::new(dirs.clone()),
        };
        let tx = this.sender.clone();
        this.wbfs_folder.connect_const(&tx, Message::WbfsDirChange);
        this
    }
    fn spawn_with_clone<'a, R, Fut>(&self, map: impl FnOnce(Self) -> Fut)
    where
        R: Send + 'static,
        Fut: Future<Output = R> + Send + 'static,
    {
        spawn(map(self.clone()))
    }
    async fn add_wbfs(&self, path: PathBuf) -> R<()> {
        let disc = nod::Disc::new(path)?;
        let id = std::str::from_utf8(disc.header().game_id.as_slice())
            .unwrap()
            .to_string();
        let name = "todo: name".to_owned();
        let cover = self.covers.get_cover(&id).await?;
        self.gamelist
            .map_mut(|mut games| games.push(Game::new(name, id, cover)));
        Ok(())
    }
    async fn on_wbfs_dir_change(&self) -> R<()> {
        let mut games = Vec::new();
        let dir = self.wbfs_folder.get();
        for entry in dir.read_dir()? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let path = entry.path();
            let Some(path) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some((start, end)) = path
                .find("[")
                .and_then(|start| Some((start, path.find("]")?)))
            else {
                continue;
            };
            let id = &path[start + 1..end];
            let name = path[..start].trim();
            let cover = self.covers.get_cover(id).await?;
            games.push(Game::new(name, id, cover))
        }
        self.gamelist.set(games);
        Ok(())
    }
}

fn spawn<Output, F>(fut: F)
where
    F: Future<Output = Output> + Send + 'static,
    Output: Send + 'static,
{
    tokio::spawn(fut);
}

type Gamelist = Dynamic<Vec<Game>>;

fn main_window(app_data: AppData) -> impl MakeWidget {
    let window_overlay = OverlayLayer::default();
    let search_term = Dynamic::new(String::new());
    let search = search_term
        .to_input()
        .expand_horizontally()
        .and("Search".small().align_left())
        .into_rows()
        .gutter(Dimension::Lp(Lp::new(0)))
        .pad();

    let gamelist = app_data.gamelist.clone();
    menu(window_overlay.clone(), app_data.clone())
        .and(search)
        .and(VirtualList::new(gamelist.map_each(|g| g.len()), move |i| {
            gamelist.get()[i].clone()
        }))
        .into_rows()
        .and(window_overlay)
        .into_layers()
}

static MAIN_WINDOW: OnceLock<WindowHandle> = OnceLock::new();
fn main_window_handle() -> &'static WindowHandle {
    MAIN_WINDOW.get().unwrap()
}

#[derive(Clone)]
enum Message {
    WbfsDirChange,
    AddWbfs(PathBuf),
}

async fn update(rx: &mut Receiver<Message>, cached_data: &CachedData, data: &AppData) -> R {
    println!("alive");
    while let Some(msg) = rx.recv().await {
        match msg {
            Message::WbfsDirChange => {
                data.on_wbfs_dir_change().await?;
                cached_data.save().await?;
            }
            Message::AddWbfs(file) => {
                data.add_wbfs(file).await?;
            }
        }
    }
    println!("bye");
    Ok(())
}

fn main() -> R {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let _tk = runtime.enter();
    let mut app = cushy::PendingApp::new(cushy::TokioRuntime::from(runtime.handle().clone()));
    let (tx, rx) = tokio::sync::mpsc::channel(16);

    // Load cached data / user preferences
    let dirs = ProjectDirs::from("dev", "diegovsky", "wii-organizer").unwrap();
    let cached = CachedData::load(&dirs);
    // Load game list as in the background
    tx.update(Message::WbfsDirChange);

    let window_title = Dynamic::new(MaybeLocalized::Text("Wii Manager".into()));
    let app_data = AppData::new(window_title.clone(), &dirs, &cached, tx);
    // app_data.wbfs_folder.set("/run/media/diegovsky/IDOSAS/wbfs".into();

    // Create shared app data
    // Create update thread
    runtime.spawn(closure! {async [mut app_data, &mut cached, &mut rx] loop {
            if let Err(e) = update(&mut rx,&mut cached,&mut app_data).await {
                eprintln!("Update error: {e:?}")
            }
        }
    });

    app.on_startup(move |app: &mut App| -> cushy::Result {
        let pending = PendingWindow::default();
        let window = pending.handle();
        MAIN_WINDOW.set(window).unwrap();

        pending
            .with_root(main_window(app_data).make_widget())
            .titled(window_title)
            .open(app)?;
        cushy::Result::Ok(())
    });

    app.run()?;

    Ok(())
}
