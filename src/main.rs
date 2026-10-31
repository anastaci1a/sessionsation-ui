use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use crossterm::{
    cursor::{Hide, Show},
    event::{
        self, DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange,
        Event, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
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
#[command(version, about = "Account and session management popup for HERDR")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,
    #[arg(long)]
    preferences: Option<PathBuf>,
}
#[derive(Subcommand)]
enum Command {
    #[command(hide = true)]
    SubmitWorker {
        #[arg(long)]
        request_id: String,
        #[arg(long)]
        journal: PathBuf,
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        binary: PathBuf,
    },
    /// Dispatches the UI operation queue after the popup closes; one per queue.
    #[command(hide = true)]
    QueueWorker {
        #[arg(long)]
        queue: PathBuf,
        #[arg(long)]
        journal: PathBuf,
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        binary: PathBuf,
    },
    Popup,
    Open,
    /// Runs a backend login plan handed over by the manager in a plugin pane.
    #[command(hide = true)]
    LoginRun,
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
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            ),
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
            PopKeyboardEnhancementFlags,
            EnableLineWrap,
            Show,
            LeaveAlternateScreen
        );
    }
}
enum Message {
    Input(Event),
    InputError(String),
    Background([u8; 3]),
    Inventory(Result<(Vec<Account>, Vec<Workspace>, Value), String>),
    Layout(sessionsation_ui::same_name::Layout),
    SameName(Result<(bool, Option<sessionsation_ui::same_name::Existing>), String>),
    TerminalRan(Result<String, String>),
    Managed(u64, Result<Value, String>),
    SessionFocused(Result<(), String>),
    LoginLaunched(Result<(), String>),
    LoginScreen(String, Result<Option<String>, String>),
    Theme(Result<(sessionsation_ui::dialog::Palette, Option<String>), String>),
    Directory(u64, Result<Listing, String>),
    Validation(u64, Result<(), String>),
    Receipt(String, Result<Value, String>),
    Focused(Result<(), String>),
    Saved(Result<(), String>),
    Created(sessionsation_ui::app::Target, PathBuf, Result<(), String>),
    Queue(Vec<sessionsation_ui::op_queue::Entry>),
}
impl Message {
    fn kind(&self) -> &'static str {
        match self {
            Message::Input(Event::Key(_)) => "key",
            Message::Input(_) => "input",
            Message::InputError(_) => "input_error",
            Message::Background(_) => "background",
            Message::Inventory(_) => "inventory",
            Message::Layout(_) => "layout",
            Message::SameName(_) => "same_name",
            Message::TerminalRan(_) => "terminal_ran",
            Message::Managed(..) => "managed",
            Message::SessionFocused(_) => "session_focused",
            Message::LoginLaunched(_) => "login_launched",
            Message::LoginScreen(..) => "login_screen",
            Message::Theme(_) => "theme",
            Message::Directory(..) => "directory",
            Message::Validation(..) => "validation",
            Message::Receipt(..) => "receipt",
            Message::Focused(_) => "focused",
            Message::Saved(_) => "saved",
            Message::Created(..) => "created",
            Message::Queue(_) => "queue",
        }
    }
}
struct Effects {
    client: Client,
    sender: mpsc::Sender<Message>,
    preferences: PathBuf,
    launch_journal: sessionsation_ui::launch_intent::Journal,
    queue: sessionsation_ui::op_queue::Store,
    initial: PathBuf,
    env: Arc<BTreeMap<String, String>>,
    directory_limit: Arc<Semaphore>,
    directory: Option<tokio::task::JoinHandle<()>>,
    validation: Option<tokio::task::JoinHandle<()>>,
}
impl Effects {
    fn run(&mut self, effects: Vec<Effect>, app: &App) {
        for effect in effects {
            if sessionsation_ui::trace::enabled() {
                sessionsation_ui::trace::event("effect", format_args!("{}", effect_kind(&effect)));
            }
            let tx = self.sender.clone();
            let client = self.client.clone();
            match effect {
                Effect::Api(token, args) => {
                    let journal = self.launch_journal.clone();
                    tokio::spawn(async move {
                        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
                        let result = match refs.as_slice() {
                            [kind @ ("launch" | "resume-launch"), "list"] => {
                                journal.list(&client, kind).await
                            }
                            [
                                kind @ ("launch" | "resume-launch"),
                                "status",
                                "--request-id",
                                id,
                            ] => journal.status(&client, kind, id).await,
                            _ => client.api(&refs, None).await,
                        }
                        .map_err(|e| e.to_string());
                        let _ = tx.send(Message::Managed(token, result)).await;
                    });
                }
                Effect::ApiInput(token, args, secret) => {
                    tokio::spawn(async move {
                        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
                        let result = client
                            .api(&refs, Some(secret.0))
                            .await
                            .map_err(|e| e.to_string());
                        let _ = tx.send(Message::Managed(token, result)).await;
                    });
                }
                Effect::FocusSession(session, direct) => {
                    tokio::spawn(async move {
                        let result = client
                            .focus_session(&session, direct)
                            .await
                            .map_err(|e| e.to_string());
                        let _ = tx.send(Message::SessionFocused(result)).await;
                    });
                }
                Effect::CreateDirectories(target, path, missing) => {
                    tokio::spawn(async move {
                        let result = tokio::task::spawn_blocking(move || {
                            for dir in &missing {
                                match std::fs::create_dir(dir) {
                                    Ok(()) => {}
                                    Err(e)
                                        if e.kind() == std::io::ErrorKind::AlreadyExists
                                            && dir.is_dir() => {}
                                    Err(e) => return Err(format!("{}: {e}", dir.display())),
                                }
                            }
                            Ok(())
                        })
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r);
                        let _ = tx.send(Message::Created(target, path, result)).await;
                    });
                }
                Effect::ReadLoginPane(key) => {
                    tokio::spawn(async move {
                        let result = client.login_screen().await.map_err(|e| e.to_string());
                        let _ = tx.send(Message::LoginScreen(key, result)).await;
                    });
                }
                Effect::LaunchLogin(argv) => {
                    tokio::spawn(async move {
                        // The plan travels only in the pane's environment, never in
                        // logs or errors; failures carry no plan text.
                        let plan = serde_json::to_string(&argv).unwrap_or_default();
                        let result = client
                            .host(&[
                                "plugin",
                                "pane",
                                "open",
                                "--plugin",
                                "sessionsation",
                                "--entrypoint",
                                "login",
                                "--placement",
                                "overlay",
                                "--focus",
                                "--env",
                                &format!("{LOGIN_ENV}={plan}"),
                            ])
                            .await
                            .map(|_| ())
                            .map_err(|_| "HERDR could not open the login pane".to_string());
                        let _ = tx.send(Message::LoginLaunched(result)).await;
                    });
                }
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
                Effect::ProbeSameName(space, name) => {
                    tokio::spawn(async move {
                        let result = client
                            .typable_same_name(&space, &name)
                            .await
                            .map_err(|e| e.to_string());
                        let _ = tx.send(Message::SameName(result)).await;
                    });
                }
                Effect::RunInTerminal(space, name, existing, request) => {
                    tokio::spawn(async move {
                        let result = async {
                            let command = sessionsation_ui::same_name::inline_command(
                                &client.binary,
                                &client.root,
                                &request,
                            )
                            .context("Invalid launch request")?;
                            client
                                .run_in_terminal(&space, &name, &existing, &command)
                                .await?;
                            Ok::<_, anyhow::Error>(existing.tab.clone())
                        }
                        .await
                        .map_err(|e| e.to_string());
                        let _ = tx.send(Message::TerminalRan(result)).await;
                    });
                }
                Effect::Start(request) => {
                    let journal = self.launch_journal.clone();
                    let id = request["request_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned();
                    // Persist the click before any asynchronous work; a separate
                    // one-shot submitter survives closure of this viewer.
                    // Only the small durable click record precedes the task;
                    // cold executable spawn must not block input/rendering.
                    tokio::task::spawn_blocking(move || {
                        let accepted = journal.accept(&request);
                        let result = match accepted {
                            Ok(()) => {
                                let result = std::env::current_exe()
                                    .map_err(anyhow::Error::from)
                                    .and_then(|exe| {
                                        journal.spawn_worker(&exe, &client, &id).map(|mut child| {
                                            // Reap completed submitters while this viewer lives.
                                            // This waiter neither owns lifecycle nor holds the
                                            // async runtime open when the popup exits.
                                            std::thread::spawn(move || {
                                                let _ = child.wait();
                                            });
                                        })
                                    });
                                if let Err(e) = &result {
                                    let _ =
                                        journal.finish(&id, &Err(anyhow::anyhow!(e.to_string())));
                                }
                                result
                            }
                            Err(e) => Err(e),
                        };
                        if let Err(e) = result {
                            let _ = tx.blocking_send(Message::Receipt(id, Err(e.to_string())));
                        }
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
    /// Apply queue edits durably, start the worker when work is pending, and
    /// send the saved queue back to the view.
    fn write_queue(&self, commands: Vec<sessionsation_ui::op_queue::Command>) {
        let store = self.queue.clone();
        let tx = self.sender.clone();
        let (client, journal) = (self.client.clone(), self.launch_journal.clone());
        tokio::task::spawn_blocking(move || {
            let Ok((_, entries)) = store.update(|entries| {
                for command in commands {
                    sessionsation_ui::op_queue::apply(entries, command);
                }
            }) else {
                return;
            };
            spawn_queue_worker(&store, &client, &journal, &entries);
            let _ = tx.blocking_send(Message::Queue(entries));
        });
    }
    fn read_queue(&self) {
        let store = self.queue.clone();
        let tx = self.sender.clone();
        let (client, journal) = (self.client.clone(), self.launch_journal.clone());
        tokio::task::spawn_blocking(move || {
            let entries = store.load();
            spawn_queue_worker(&store, &client, &journal, &entries);
            let _ = tx.blocking_send(Message::Queue(entries));
        });
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
                // The tab/pane snapshot only feeds the same-name hint; a failure
                // leaves the last one in place.
                if let Ok(layout) = client.layout().await {
                    let _ = tx.send(Message::Layout(layout)).await;
                }
                Ok::<_, anyhow::Error>((
                    serde_json::from_value(accounts)?,
                    serde_json::from_value(spaces["workspaces"].clone())?,
                    capabilities,
                ))
            }
            .await
            .map_err(|e| e.to_string());
            let _ = tx.send(Message::Inventory(result)).await;
        });
    }
    fn receipt(&self, id: String, resume: bool) {
        let journal = self.launch_journal.clone();
        let tx = self.sender.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let result = journal
                .status(
                    &client,
                    if resume { "resume-launch" } else { "launch" },
                    &id,
                )
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(Message::Receipt(id, result)).await;
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
/// Start a detached queue worker when entries wait or run. It exits at once if
/// another worker owns the queue, and outlives this popup otherwise.
fn spawn_queue_worker(
    store: &sessionsation_ui::op_queue::Store,
    client: &Client,
    journal: &sessionsation_ui::launch_intent::Journal,
    entries: &[sessionsation_ui::op_queue::Entry],
) {
    use std::os::unix::process::CommandExt;
    if !entries
        .iter()
        .any(|e| e.state != sessionsation_ui::op_queue::State::Failed)
        || store.worker_running()
    {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut command = std::process::Command::new(exe);
    command
        .arg("queue-worker")
        .arg("--queue")
        .arg(&store.path)
        .arg("--journal")
        .arg(&journal.directory)
        .arg("--root")
        .arg(&store.root)
        .arg("--binary")
        .arg(&client.binary)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: setsid only detaches the child from this terminal session.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    if let Ok(mut child) = command.spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}
fn effect_kind(effect: &Effect) -> String {
    match effect {
        Effect::Api(token, args) => format!("api#{token} {}", args.join(" ")),
        // Arguments only: the stdin secret is never part of a trace.
        Effect::ApiInput(token, args, _) => format!("api#{token} {}", args.join(" ")),
        Effect::FocusSession(..) => "focus_session".into(),
        Effect::LaunchLogin(_) => "launch_login".into(),
        Effect::ReadLoginPane(_) => "read_login_pane".into(),
        Effect::CreateDirectories(..) => "create_directories".into(),
        Effect::Inspect(..) => "inspect".into(),
        Effect::Validate(..) => "validate".into(),
        Effect::Start(_) => "start".into(),
        Effect::ProbeSameName(..) => "probe_same_name".into(),
        Effect::RunInTerminal(..) => "run_in_terminal".into(),
        Effect::Save => "save".into(),
        Effect::Exit => "exit".into(),
    }
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
/// Effect tasks (backend JSON parsing, journal fsyncs, child spawns) run on worker
/// threads. The thread that reads input and redraws only drives the select loop, so
/// slow or blocking work in a task can never stall key handling or rendering.
fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
}
fn main() -> Result<()> {
    runtime()?.block_on(run())
}
async fn run() -> Result<()> {
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
        Command::LoginRun => login_run(),
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
        Command::SubmitWorker {
            request_id,
            journal,
            root,
            binary,
        } => {
            let client = Client {
                binary,
                root: root.clone(),
                herdr: PathBuf::new(),
                socket: PathBuf::new(),
                session: String::new(),
            };
            sessionsation_ui::launch_intent::Journal {
                directory: journal,
                root,
            }
            .submit(&client, &request_id)
            .await
        }
        Command::QueueWorker {
            queue,
            journal,
            root,
            binary,
        } => {
            let client = Client {
                binary,
                root: root.clone(),
                herdr: PathBuf::new(),
                socket: PathBuf::new(),
                session: String::new(),
            };
            let store = sessionsation_ui::op_queue::Store {
                path: queue,
                root: root.clone(),
            };
            sessionsation_ui::op_queue::worker::run(
                store,
                client,
                sessionsation_ui::launch_intent::Journal {
                    directory: journal,
                    root,
                },
            )
            .await
        }
        Command::Popup => popup(args.preferences).await,
    }
}
const LOGIN_ENV: &str = "SESSIONSATION_LOGIN_ARGV";
/// Runs the backend-owned login plan on this pane's terminal. The plan is read from
/// the environment, removed from the child's environment and never printed.
fn login_run() -> Result<()> {
    let plan = std::env::var(LOGIN_ENV).context("No login plan was provided")?;
    let argv: Vec<String> = serde_json::from_str(&plan).context("Invalid login plan")?;
    ensure!(
        !argv.is_empty() && argv.iter().all(|a| !a.is_empty() && !a.contains('\0')),
        "Invalid login plan"
    );
    println!("Sessionsation provider login. Complete the sign-in below.\n");
    let status = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .env_remove(LOGIN_ENV)
        .status();
    let (message, wait) = login_outcome(&status);
    println!("\n{message}");
    if wait {
        let _ = std::io::stdin().read_line(&mut String::new());
    } else {
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
    Ok(())
}
async fn popup(preferences: Option<PathBuf>) -> Result<()> {
    ensure!(
        std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        "A terminal is required; use sessionsation api for machine access"
    );
    let client = Client::load()?;
    if let Err(error) = client.require_contract().await {
        // The popup pane closes on exit: say why and wait for a key first.
        eprintln!("\nSessionsation cannot start.\n\n{error:#}\n\nPress Enter to close.");
        let _ = std::io::stdin().read_line(&mut String::new());
        return Err(error);
    }
    let prefixes = sessionsation_ui::host_keys::Prefixes::load();
    let pref =
        preferences.unwrap_or_else(|| sessionsation_ui::client::preferences_path(&config_path()));
    let prefs = Preferences::load(&pref)?;
    let context = std::env::var("HERDR_PLUGIN_CONTEXT_JSON").ok();
    let (origin_space, origin_cwd) = sessionsation_ui::client::origin(context.as_deref())?;
    let initial = origin_cwd
        .or_else(|| std::env::var_os("HERDR_ACTIVE_PANE_CWD").map(PathBuf::from))
        .unwrap_or(std::env::current_dir()?);
    let space = origin_space.or_else(|| {
        if context.is_none() {
            std::env::var("HERDR_ACTIVE_WORKSPACE_ID").ok()
        } else {
            None
        }
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
    std::io::stderr().write_all(b"\x1b]11;?\x1b\\")?;
    std::io::stderr().flush()?;
    std::thread::spawn(move || {
        let mut colors = sessionsation_ui::terminal_color::BackgroundReply::default();
        loop {
            for e in colors.expire() {
                if input.blocking_send(Message::Input(e)).is_err() {
                    return;
                }
            }
            let polled = Instant::now();
            let ready = event::poll(Duration::from_millis(50)).unwrap_or(true);
            let poll_us = polled.elapsed().as_micros();
            if poll_us > 80_000 {
                sessionsation_ui::trace::event("poll_slow", format_args!("us={poll_us}"));
            }
            if !ready {
                continue;
            }
            match event::read() {
                Ok(e) => {
                    sessionsation_ui::trace::event("input", format_args!("{e:?}"));
                    let (events, color) = colors.event(e);
                    if let Some(color) = color
                        && input.blocking_send(Message::Background(color)).is_err()
                    {
                        break;
                    }
                    for e in events {
                        if input.blocking_send(Message::Input(e)).is_err() {
                            return;
                        }
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
        let mut rotation = 0;
        while let Ok(canvas) = frames.recv() {
            let diffed = Instant::now();
            // Pacing keeps star-sweep bursts from backing up a slow terminal link
            // and, through it, HERDR's key forwarding. Frames arrive every 50 ms,
            // so deferred stars catch up without a dedicated timer.
            let (delta, shown) = canvas.paced_diff(
                previous.as_ref(),
                sessionsation_ui::render::AMBIENT_CELLS_PER_FRAME,
                &mut rotation,
            );
            let diff_us = diffed.elapsed().as_micros();
            let written = Instant::now();
            if std::io::stderr().write_all(delta.as_bytes()).is_err() {
                break;
            }
            let _ = std::io::stderr().flush();
            sessionsation_ui::trace::event(
                "write",
                format_args!(
                    "bytes={} diff_us={diff_us} write_us={}",
                    delta.len(),
                    written.elapsed().as_micros()
                ),
            );
            previous = Some(shown);
        }
    });
    let cache_path = pref.with_file_name("state-cache.json");
    let cache_root = client.root.clone();
    if let Some(sessions) = sessionsation_ui::state_cache::load(&cache_path, &cache_root) {
        app.manager.show_cached(&sessions);
    }
    let dismissed_path = pref.with_file_name("dismissed-operations.json");
    if let Some(operations) =
        sessionsation_ui::state_cache::load_dismissed(&dismissed_path, &cache_root)
    {
        app.manager.dismissed = operations.into_iter().collect();
    }
    let queue_store = sessionsation_ui::op_queue::Store::beside(&pref, &client.root);
    app.manager.set_queue(queue_store.load());
    let accounts_cache_path = pref.with_file_name("accounts-cache.json");
    let loaded = Instant::now();
    if let Some(accounts) =
        sessionsation_ui::state_cache::load_accounts(&accounts_cache_path, &cache_root)
    {
        app.manager.show_cached_accounts(&accounts);
        sessionsation_ui::trace::event(
            "accounts_cached",
            format_args!("us={}", loaded.elapsed().as_micros()),
        );
    }
    let mut effects = Effects {
        client: client.clone(),
        sender: tx.clone(),
        launch_journal: sessionsation_ui::launch_intent::Journal {
            directory: pref.with_file_name("launch-intents"),
            root: client.root.clone(),
        },
        queue: queue_store.clone(),
        preferences: pref,
        initial,
        env: Arc::new(std::env::vars().collect()),
        directory_limit: Arc::new(Semaphore::new(1)),
        directory: None,
        validation: None,
    };
    let theme_sender = tx.clone();
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(sessionsation_ui::host_theme::load)
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));
        let _ = theme_sender.send(Message::Theme(result)).await;
    });
    let (w, h) = terminal::size()?;
    let mut dimensions = (w as usize, h as usize);
    let mut field = Field::new(dimensions.0, dimensions.1, now() as u64);
    let started = Instant::now();
    let mut frame = tokio::time::interval(Duration::from_millis(50));
    frame.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut refresh = tokio::time::interval(Duration::from_secs(5));
    let mut session_refresh = tokio::time::interval(Duration::from_secs(1));
    session_refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut receipt = tokio::time::interval(Duration::from_secs(1));
    let mut inventory_pending = false;
    let mut receipt_pending: Option<String> = None;
    let mut term_signal =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut hup_signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;
    let traced = sessionsation_ui::trace::enabled();
    while !app.exit {
        if app.dirty {
            let painted = Instant::now();
            let canvas = app.paint(
                &mut field,
                started.elapsed().as_millis() as u64,
                dimensions.0,
                dimensions.1,
            );
            let paint_us = painted.elapsed().as_micros();
            let sent = output.try_send(canvas).is_ok();
            if sent {
                app.dirty = false;
            }
            if traced {
                sessionsation_ui::trace::event("paint", format_args!("us={paint_us} sent={sent}"));
            }
        }
        let mut next = vec![];
        let waited = Instant::now();
        let arm: &str;
        let woke: Instant;
        tokio::select! {
         _=term_signal.recv()=>{arm="term";woke=Instant::now();app.exit=true;},_ = hup_signal.recv()=>{arm="hup";woke=Instant::now();app.exit=true;},
         _=frame.tick()=>{arm="frame";woke=Instant::now();next=app.flush_navigation();app.dirty=true;},
         _=refresh.tick(),if !inventory_pending=>{arm="refresh";woke=Instant::now();inventory_pending=true;effects.inventory();let intents=app.manager.refresh_poll();next=app.management(intents);},
         _=session_refresh.tick()=>{arm="session_refresh";woke=Instant::now();next=app.refresh_live();effects.read_queue();},
         _=receipt.tick(),if app.request.is_some()&&!app.success&&receipt_pending.as_deref()!=app.request.as_ref().and_then(|r|r["request_id"].as_str())=>{arm="receipt";woke=Instant::now();receipt_pending=app.request.as_ref().and_then(|r|r["request_id"].as_str()).map(str::to_owned);effects.receipt(app.request.as_ref().unwrap()["request_id"].as_str().unwrap().into(),app.request.as_ref().unwrap()["session_id"].is_string());},
         message=rx.recv()=>{woke=Instant::now();arm=message.as_ref().map_or("closed",Message::kind);match message.context("Input channel closed")? {
         Message::Background(color)=>{app.terminal_background=color;app.dirty=true;},
         Message::Input(Event::Key(k))=>{if prefixes.matches(k){app.exit=true;}else{sessionsation_ui::widgets::restart_selection_marquee(started.elapsed().as_millis() as u64);next=app.key(k);}},
          Message::Input(Event::Paste(s))=>{next=app.paste(&s);},
          Message::Input(Event::Resize(w,h))=>{dimensions=(w as usize,h as usize);app.dirty=true;},
          Message::Input(Event::FocusGained)=>{app.manager.visible=true;let intents=app.manager.read_sessions();next=app.management(intents);app.dirty=true;},Message::Input(Event::FocusLost)=>{app.manager.visible=false;},Message::Input(_)=>{},
          Message::InputError(e)=>{app.error=e;app.exit=true;},
          Message::Inventory(result)=>{inventory_pending=false;match result{Ok((accounts,spaces,capabilities))=>{if let Ok(saved)=serde_json::to_value(&accounts){let (path,root)=(accounts_cache_path.clone(),cache_root.clone());tokio::task::spawn_blocking(move||{let _=sessionsation_ui::state_cache::save_accounts(&path,&root,&saved);});}app.launch_supported=capabilities["launch"]["version"]==1;let changed=app.manager.capabilities!=capabilities;app.manager.capabilities=capabilities;next=app.inventory(accounts,spaces,now());if changed{let intents=app.manager.refresh();next.extend(app.management(intents));}
          if !app.launch_supported{app.error="Backend launch API is not installed; creation disabled".into();}},Err(e)=>{app.manager.note=e.clone();app.error=e;app.draft.name_valid=false;}}app.dirty=true;},
          Message::Layout(layout)=>app.set_layout(layout),
          Message::SameName(result)=>{next=app.same_name_probed(result);},
          Message::TerminalRan(result)=>{if let Some(tab)=app.terminal_ran(result){effects.focus(tab);}},
          Message::Managed(token,result)=>{let intents=app.manager.response(token,result);next=app.management(intents);if let Some(sessions)=app.manager.cache_out.take(){let (path,root)=(cache_path.clone(),cache_root.clone());tokio::task::spawn_blocking(move||{let _=sessionsation_ui::state_cache::save(&path,&root,&sessions);});}},
          Message::LoginLaunched(result)=>{let intents=app.manager.login_launched(result);next=app.management(intents);app.dirty=true;},
          Message::LoginScreen(key,result)=>{let intents=app.manager.login_screen(key,result);next=app.management(intents);app.dirty=true;},
          Message::SessionFocused(result)=>{match result{Ok(())=>app.exit=true,Err(e)=>{let intents=app.manager.failure(e);next=app.management(intents);}}app.dirty=true;},
          Message::Theme(result)=>{match result{Ok((palette,note))=>{app.manager.palette=palette;if let Some(note)=note{app.manager.note=note;}},Err(e)=>app.manager.note=e}app.dirty=true;},
          Message::Created(target,path,result)=>{next=app.directories_created(target,path,result);},
          Message::Queue(entries)=>{app.manager.set_queue(entries);app.dirty=true;},
          Message::Directory(revision,result)=>app.listing(revision,result),
          Message::Validation(revision,result)=>app.validation(revision,result),
          Message::Receipt(id,result)=>{if receipt_pending.as_deref()==Some(id.as_str()){receipt_pending=None;}
          if app.request.as_ref().and_then(|r|r["request_id"].as_str()) != Some(id.as_str()) { continue; } match result {
           Ok(r)=>{if matches!(r["phase"].as_str(),Some("running"|"failed"|"cancelled"|"recovered"|"complete")){app.manager.launch_settled(&id);}match r["phase"].as_str().unwrap_or("uncertain") {
            "running" if !app.success=>{app.success=true;if app.launch_background {app.manager.note="Session is running; open Operations or Focus tab when ready.".into();app.prefs.last_account=app.request.as_ref().and_then(|r|r["account_id"].as_str().map(str::to_owned));next.push(Effect::Save);}else if let Some(tab)=r["tab_id"].as_str(){effects.focus(tab.into());}else{app.error="Session running; receipt has no tab identity. Esc closes.".into();}},
            "failed" | "cancelled" | "recovered"=>{app.request=None;app.error=format!("Launch failed: {}. Esc closes; no automatic retry.",sessionsation_ui::messages::describe(r["detail"].as_str().unwrap_or("inspect launch status")).trim_end_matches('.'));},
            phase=>{app.error=sessionsation_ui::messages::join(&sessionsation_ui::messages::phase(phase),r["detail"].as_str().unwrap_or("waiting for confirmed handoff"));},
           }},Err(e)=>{app.error=sessionsation_ui::app::reconciliation_message(&e);}
          }
          // The launch dialog already says this; the bottom lane does not repeat it.
          if !(app.success||app.launch_dialog&&app.manager.dialog.is_some()) {app.manager.note=app.error.clone();}
          if app.launch_dialog&&let Some(d)=&mut app.manager.dialog{d.body=vec![app.error.clone()];if app.success || app.error.starts_with("Launch failed:"){d.finish();}}app.dirty=true;},
          Message::Focused(result)=>{match result { Ok(())=>{app.prefs.last_account=app.draft.account.clone();next.push(Effect::Save);},Err(e)=>{app.error=format!("Session is running; tab focus failed: {e}. Esc closes.");}}app.dirty=true;},
          Message::Saved(result)=>{if let Err(e)=result{app.error=format!("Preferences were not saved: {e}");if !app.success{app.page=Page::Config;}app.dirty=true;}else if app.success && !app.launch_background {app.exit=true;}},
         }}
        }
        let queued = std::mem::take(&mut app.manager.queue_out);
        if !queued.is_empty() {
            effects.write_queue(queued);
        }
        if let Some(operations) = app.manager.dismissed_out.take() {
            let (path, root) = (dismissed_path.clone(), cache_root.clone());
            tokio::task::spawn_blocking(move || {
                let _ = sessionsation_ui::state_cache::save_dismissed(&path, &root, &operations);
            });
        }
        if traced {
            sessionsation_ui::trace::event(
                "arm",
                format_args!(
                    "{arm} wait_us={} apply_us={} effects={}",
                    woke.duration_since(waited).as_micros(),
                    woke.elapsed().as_micros(),
                    next.len()
                ),
            );
            if arm == "frame" {
                sessionsation_ui::trace::flush();
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

/// What the login pane says for the child's result, and whether it waits for
/// Enter. A successful login closes by itself; a failure stays readable.
fn login_outcome(status: &std::io::Result<std::process::ExitStatus>) -> (String, bool) {
    match status {
        Ok(s) if s.success() => ("Login complete. This pane closes automatically.".into(), false),
        Ok(_) => (
            "Login did not complete. Press Enter to close this pane, then choose Login on the account page to retry.".into(),
            true,
        ),
        Err(e) => (
            format!(
                "Could not start the login command ({}). Press Enter to close this pane.",
                e.kind()
            ),
            true,
        ),
    }
}

#[cfg(test)]
mod responsiveness_tests {
    use std::time::{Duration, Instant};
    /// The input loop must keep ticking while a slow backend call (2 s) and a
    /// blocking synchronous task (journal fsync, large JSON parse) are in flight.
    #[test]
    fn input_loop_stays_responsive_during_slow_backend_and_blocking_work() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("slow-backend");
        std::fs::write(
            &bin,
            "#!/bin/sh\nsleep 2\nprintf '{\"api_version\":1,\"ok\":true,\"result\":{}}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let client = sessionsation_ui::client::Client {
            binary: bin,
            root: dir.path().into(),
            herdr: dir.path().join("herdr"),
            socket: dir.path().join("sock"),
            session: "s".into(),
        };
        let rt = super::runtime().unwrap();
        let worst = rt.block_on(async move {
            let (tx, mut rx) = tokio::sync::mpsc::channel::<Instant>(64);
            let api = tokio::spawn(async move { client.api(&["state"], None).await.is_ok() });
            let blocker = tokio::spawn(async { std::thread::sleep(Duration::from_millis(600)) });
            // Stand-in for the input thread: a key event every 20 ms.
            tokio::task::spawn_blocking(move || {
                for _ in 0..90 {
                    std::thread::sleep(Duration::from_millis(20));
                    if tx.blocking_send(Instant::now()).is_err() {
                        break;
                    }
                }
            });
            let mut worst = Duration::ZERO;
            let mut selected = 0;
            while let Some(sent) = rx.recv().await {
                selected += 1; // Down arrow moved the selection
                worst = worst.max(sent.elapsed());
                if selected == 90 {
                    break;
                }
            }
            assert!(!api.is_finished() || api.await.unwrap());
            blocker.await.unwrap();
            worst
        });
        assert!(worst < Duration::from_millis(100), "key latency {worst:?}");
    }
}

#[cfg(test)]
mod login_tests {
    use super::login_outcome;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    #[test]
    fn success_closes_itself_and_failure_waits_for_enter() {
        let (m, wait) = login_outcome(&Ok(ExitStatus::from_raw(0)));
        assert!(m.starts_with("Login complete") && !wait);
        let (m, wait) = login_outcome(&Ok(ExitStatus::from_raw(1 << 8)));
        assert!(m.starts_with("Login did not complete") && wait);
        let err = std::io::Error::from(std::io::ErrorKind::NotFound);
        let (m, wait) = login_outcome(&Err(err));
        assert!(m.contains("not found") && wait);
    }
}
