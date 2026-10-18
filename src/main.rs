use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange,
        Event, KeyEventKind,
    },
    execute,
    terminal::{self, DisableLineWrap, EnableLineWrap, EnterAlternateScreen, LeaveAlternateScreen},
};
use serde_json::{Value, json};
use sessionsation_ui::{
    app::{App, Effect, Page},
    client::{Account, Client, Workspace, config_path},
    directory::{self, Listing},
    preferences::Preferences,
    render::{Canvas, Field},
};
use std::{
    collections::BTreeMap,
    io::{IsTerminal, Write},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Semaphore, mpsc};
#[derive(Parser)]
#[command(
    version,
    about = "New-session HERDR popup; no existing-agent management"
)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
    #[arg(long)]
    preferences: Option<PathBuf>,
}
#[derive(Subcommand)]
enum Command {
    Popup,
    Open,
    Demo {
        #[arg(long, default_value_t = 80)]
        width: usize,
        #[arg(long, default_value_t = 24)]
        height: usize,
        #[arg(long, default_value_t = 4400)]
        time: u64,
    },
    Benchmark,
}
struct Terminal;
impl Terminal {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let t = Self;
        execute!(
            std::io::stderr(),
            EnterAlternateScreen,
            Hide,
            EnableBracketedPaste,
            EnableFocusChange,
            DisableLineWrap
        )?;
        Ok(t)
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(
            std::io::stderr(),
            DisableBracketedPaste,
            DisableFocusChange,
            EnableLineWrap,
            Show,
            LeaveAlternateScreen
        );
    }
}
enum Message {
    Input(Event),
    InputError(String),
    Inventory(Result<(Vec<Account>, Vec<Workspace>, bool), String>),
    Directory(u64, Result<Listing, String>),
    Validation(u64, Result<(), String>),
    Receipt(Result<Value, String>),
    Focused(Result<(), String>),
    Saved(Result<(), String>),
}
struct Effects {
    client: Client,
    sender: mpsc::Sender<Message>,
    preferences: PathBuf,
    initial: PathBuf,
    env: Arc<BTreeMap<String, String>>,
    directory_limit: Arc<Semaphore>,
    directory: Option<tokio::task::JoinHandle<()>>,
    validation: Option<tokio::task::JoinHandle<()>>,
}
impl Effects {
    fn run(&mut self, effects: Vec<Effect>, app: &App) {
        for effect in effects {
            let tx = self.sender.clone();
            let client = self.client.clone();
            match effect {
                Effect::Inspect(revision, input) => {
                    if let Some(t) = self.directory.take() {
                        t.abort();
                    }
                    let env = self.env.clone();
                    let initial = self.initial.clone();
                    let semaphore = self.directory_limit.clone();
                    self.directory = Some(tokio::spawn(async move {
                        let Ok(permit) = semaphore.acquire_owned().await else {
                            return;
                        };
                        let result = tokio::task::spawn_blocking(move || {
                            let _permit = permit;
                            directory::inspect(&input, &initial, &env).map_err(|e| e.to_string())
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()));
                        let _ = tx.send(Message::Directory(revision, result)).await;
                    }));
                }
                Effect::Validate(revision, request) => {
                    if let Some(t) = self.validation.take() {
                        t.abort();
                    }
                    self.validation = Some(tokio::spawn(async move {
                        let result = client
                            .launch("validate", request)
                            .await
                            .map(|_| ())
                            .map_err(|e| e.to_string());
                        let _ = tx.send(Message::Validation(revision, result)).await;
                    }));
                }
                Effect::Start(request) => {
                    tokio::spawn(async move {
                        let result = client
                            .launch("start", request)
                            .await
                            .map_err(|e| e.to_string());
                        let _ = tx.send(Message::Receipt(result)).await;
                    });
                }
                Effect::Save => {
                    let p = self.preferences.clone();
                    let prefs = app.prefs.clone();
                    tokio::spawn(async move {
                        let result = tokio::task::spawn_blocking(move || {
                            prefs.save(&p).map_err(|e| e.to_string())
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()));
                        let _ = tx.send(Message::Saved(result)).await;
                    });
                }
                Effect::Exit => {}
            }
        }
    }
    fn inventory(&self) {
        let tx = self.sender.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let result = async {
                let (accounts, spaces, capabilities) = tokio::try_join!(
                    client.api(&["account", "list"], None),
                    client.host(&["workspace", "list"]),
                    client.api(&["capabilities"], None)
                )?;
                Ok::<_, anyhow::Error>((
                    serde_json::from_value(accounts)?,
                    serde_json::from_value(spaces["workspaces"].clone())?,
                    capabilities["launch"]["version"] == 1,
                ))
            }
            .await
            .map_err(|e| e.to_string());
            let _ = tx.send(Message::Inventory(result)).await;
        });
    }
    fn receipt(&self, id: String) {
        let tx = self.sender.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let result = client
                .api(&["launch", "status", "--request-id", &id], None)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(Message::Receipt(result)).await;
        });
    }
    fn focus(&self, tab: String) {
        let tx = self.sender.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let result = client
                .host(&["tab", "focus", &tab])
                .await
                .map(|_| ())
                .map_err(|e| e.to_string());
            let _ = tx.send(Message::Focused(result)).await;
        });
    }
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let args = Args::parse();
    match args.command.unwrap_or(Command::Popup) {
        Command::Demo {
            width,
            height,
            time,
        } => {
            let mut a = App::new(
                Preferences::default(),
                "/projects".into(),
                "demo".into(),
                json!({}),
                None,
            );
            a.page = Page::Summary;
            a.draft.name = "example".into();
            let mut f = Field::new(width, height, 1967);
            for tick in (0..time).step_by(50) {
                f.paint(tick);
            }
            print!("{}", a.paint(&mut f, time, width, height).diff(None));
            Ok(())
        }
        Command::Benchmark => {
            benchmark();
            Ok(())
        }
        Command::Open => {
            let client = Client::load()?;
            client
                .host(&[
                    "plugin",
                    "pane",
                    "open",
                    "--plugin",
                    "sessionsation",
                    "--entrypoint",
                    "launcher",
                ])
                .await?;
            Ok(())
        }
        Command::Popup => popup(args.preferences).await,
    }
}
async fn popup(preferences: Option<PathBuf>) -> Result<()> {
    ensure!(
        std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        "A terminal is required; use sessionsation api for machine access"
    );
    let client = Client::load()?;
    let pref = preferences.unwrap_or_else(|| config_path().with_file_name("preferences.json"));
    let prefs = Preferences::load(&pref)?;
    let (origin_space, origin_cwd) = sessionsation_ui::client::origin(
        std::env::var("HERDR_PLUGIN_CONTEXT_JSON").ok().as_deref(),
    )?;
    let initial = origin_cwd
        .or_else(|| std::env::var_os("HERDR_ACTIVE_PANE_CWD").map(PathBuf::from))
        .unwrap_or(std::env::current_dir()?);
    let space = origin_space.or_else(|| {
        std::env::var("HERDR_ACTIVE_WORKSPACE_ID")
            .or_else(|_| std::env::var("HERDR_WORKSPACE_ID"))
            .ok()
    });
    let mut app = App::new(
        prefs,
        initial.clone(),
        client.instance(),
        client.host_value(),
        space,
    );
    let (tx, mut rx) = mpsc::channel(64);
    let input = tx.clone();
    let terminal = Terminal::enter()?;
    std::thread::spawn(move || {
        loop {
            if !event::poll(Duration::from_millis(50)).unwrap_or(true) {
                continue;
            }
            match event::read() {
                Ok(e) => {
                    if input.blocking_send(Message::Input(e)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = input.blocking_send(Message::InputError(e.to_string()));
                    break;
                }
            }
        }
    });
    // The output worker owns deltas. Dropping queued frames cannot corrupt its baseline.
    let (output, frames) = std::sync::mpsc::sync_channel::<Canvas>(1);
    let writer = std::thread::spawn(move || {
        let mut previous = None;
        while let Ok(canvas) = frames.recv() {
            let delta = canvas.diff(previous.as_ref());
            if std::io::stderr().write_all(delta.as_bytes()).is_err() {
                break;
            }
            let _ = std::io::stderr().flush();
            previous = Some(canvas);
        }
    });
    let mut effects = Effects {
        client: client.clone(),
        sender: tx.clone(),
        preferences: pref,
        initial,
        env: Arc::new(std::env::vars().collect()),
        directory_limit: Arc::new(Semaphore::new(1)),
        directory: None,
        validation: None,
    };
    let (w, h) = terminal::size()?;
    let mut dimensions = (w as usize, h as usize);
    let mut field = Field::new(dimensions.0, dimensions.1, now() as u64);
    let started = Instant::now();
    let mut frame = tokio::time::interval(Duration::from_millis(50));
    frame.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut refresh = tokio::time::interval(Duration::from_secs(5));
    let mut receipt = tokio::time::interval(Duration::from_secs(1));
    let mut inventory_pending = false;
    let mut receipt_pending = false;
    let mut term_signal =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut hup_signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;
    while !app.exit {
        if app.dirty {
            let canvas = app.paint(
                &mut field,
                started.elapsed().as_millis() as u64,
                dimensions.0,
                dimensions.1,
            );
            if output.try_send(canvas).is_ok() {
                app.dirty = false;
            }
        }
        let mut next = vec![];
        tokio::select! {
         _=term_signal.recv()=>{app.exit=true;},_ = hup_signal.recv()=>{app.exit=true;},
         _=frame.tick()=>{app.expire_prefill(now());app.dirty=true;},
         _=refresh.tick(),if !inventory_pending&&app.request.is_none()=>{inventory_pending=true;effects.inventory();},
         _=receipt.tick(),if app.request.is_some()&&!app.success&&!receipt_pending=>{receipt_pending=true;effects.receipt(app.request.as_ref().unwrap()["request_id"].as_str().unwrap().into());},
         message=rx.recv()=>match message.context("Input channel closed")? {
          Message::Input(Event::Key(k))if k.kind!=KeyEventKind::Release=>{next=app.key(k);},
          Message::Input(Event::Paste(s))=>{next=app.paste(&s);},
          Message::Input(Event::Resize(w,h))=>{dimensions=(w as usize,h as usize);app.dirty=true;},
          Message::Input(Event::FocusGained)=>{app.dirty=true;},Message::Input(_)=>{},
          Message::InputError(e)=>{app.error=e;app.exit=true;},
          Message::Inventory(result)=>{inventory_pending=false;match result{Ok((accounts,spaces,supported))=>{app.launch_supported=supported;next=app.inventory(accounts,spaces,now());if !supported{app.error="Backend launch API is not installed; creation disabled".into();}},Err(e)=>{app.error=e;app.draft.name_valid=false;}}app.dirty=true;},
          Message::Directory(revision,result)=>app.listing(revision,result),
          Message::Validation(revision,result)=>app.validation(revision,result),
          Message::Receipt(result)=>{receipt_pending=false;match result {
           Ok(r)=>match r["phase"].as_str().unwrap_or("uncertain") {
            "running" if !app.success=>{app.success=true;if let Some(tab)=r["tab_id"].as_str(){effects.focus(tab.into());}else{app.error="Session running; receipt has no tab identity. Esc closes.".into();}},
            "failed"=>{app.error=format!("Launch failed: {}. Esc closes; no automatic retry.",r["detail"].as_str().unwrap_or("inspect launch status"));},
            phase=>{app.error=format!("{phase}: {}",r["detail"].as_str().unwrap_or("waiting for confirmed handoff"));},
           },Err(e)=>{app.error=format!("{e}; reconciling original request");}
          }app.dirty=true;},
          Message::Focused(result)=>{match result { Ok(())=>{app.prefs.last_account=app.draft.account.clone();next.push(Effect::Save);},Err(e)=>{app.error=format!("Session is running; tab focus failed: {e}. Esc closes.");}}app.dirty=true;},
          Message::Saved(result)=>{if let Err(e)=result{app.error=format!("Preferences were not saved: {e}");if !app.success{app.page=Page::Config;}app.dirty=true;}else if app.success {app.exit=true;}},
         }
        }
        effects.run(next, &app);
    }
    drop(output);
    let _ = writer.join();
    drop(terminal);
    if let Some(request) = app.request {
        eprintln!(
            "Launch request: {}",
            request["request_id"].as_str().unwrap_or("")
        );
    }
    Ok(())
}
fn benchmark() {
    for (w, h) in [(80, 24), (160, 48)] {
        let mut a = App::new(
            Preferences::default(),
            "/projects".into(),
            "demo".into(),
            json!({}),
            None,
        );
        a.page = Page::Summary;
        let mut f = Field::new(w, h, 1967);
        let mut old = None;
        let mut samples = vec![];
        let mut bytes = 0;
        for now in (0..30000).step_by(50) {
            let start = Instant::now();
            let canvas = a.paint(&mut f, now, w, h);
            let output = canvas.diff(old.as_ref());
            bytes += output.len();
            old = Some(canvas);
            samples.push(start.elapsed().as_secs_f64() * 1000.);
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "{}",
            json!({"width":w,"height":h,"samples":samples.len(),"p50_ms":samples[samples.len()/2],"p95_ms":samples[samples.len()*95/100],"max_ms":samples.last(),"mean_bytes":bytes/samples.len()})
        );
    }
}
