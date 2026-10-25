use serde_json::{Value, json};
use sessionsation_ui::{client::Client, launch_intent::Journal};
use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

fn setup(mode: &str) -> (tempfile::TempDir, Client, Journal, Value) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let cli = root.join("backend");
    fs::write(&cli,r#"#!/usr/bin/env python3
import json,pathlib,sys,time
root=pathlib.Path(__file__).parent
kind='resume-launch' if 'resume-launch' in sys.argv else 'launch'
action=sys.argv[sys.argv.index(kind)+1]
with (root/'calls').open('a') as f:f.write(action+'\n')
def ok(v):print(json.dumps({'api_version':1,'ok':True,'result':v}))
def fail(code):
 print(json.dumps({'api_version':1,'ok':False,'error':{'code':code,'message':'fixture'}}),file=sys.stderr);sys.exit(1)
mode=(root/'mode').read_text()
if action=='validate':
 if mode=='reject':fail('INVALID_SESSION_NAME')
 if mode=='validate_wait':
  (root/'waiting').touch()
  while not (root/'release').exists():time.sleep(.01)
 ok({'valid':True})
elif action=='start':
 request=json.load(sys.stdin)
 if mode=='before':
  (root/'waiting').touch()
  while not (root/'release').exists():time.sleep(.01)
 receipt={'request':request,'phase':'admitted'}
 (root/'receipt').write_text(json.dumps(receipt))
 if mode=='after':
  (root/'waiting').touch()
  while not (root/'release').exists():time.sleep(.01)
 if mode=='lost':print('broken');sys.exit(1)
 ok(receipt)
elif action=='list':ok([json.loads((root/'receipt').read_text())] if (root/'receipt').exists() else [])
elif action=='status':
 if (root/'receipt').exists():ok(json.loads((root/'receipt').read_text()))
 else:fail('LAUNCH_NOT_FOUND')
"#).unwrap();
    fs::set_permissions(&cli, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(root.join("mode"), mode).unwrap();
    let client = Client {
        binary: cli,
        root: root.into(),
        herdr: "unused".into(),
        socket: "unused".into(),
        session: "fixture".into(),
    };
    let journal = Journal {
        directory: root.join("ui-intents"),
        root: root.into(),
    };
    let request = json!({"request_id":uuid::Uuid::new_v4().to_string(),"name":"fixture","directory":{"create":[]}});
    (tmp, client, journal, request)
}
async fn wait(path: &std::path::Path) {
    let end = std::time::Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < end,
            "fixture deadline: {}",
            path.display()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
#[tokio::test]
async fn exit_before_submission_is_visible_and_read_only_reconciliation_never_resends() {
    let (_tmp, client, journal, request) = setup("before");
    journal.accept(&request).unwrap();
    let id = request["request_id"].as_str().unwrap();
    let reopened = journal.clone();
    let rows = reopened.list(&client, "launch").await.unwrap();
    assert_eq!(rows[0]["phase"], "uncertain");
    assert_eq!(rows[0]["request"]["request_id"], id);
    reopened.list(&client, "launch").await.unwrap();
    assert!(reopened.accept(&request).is_err());
    let calls = fs::read_to_string(client.root.join("calls")).unwrap();
    assert_eq!(calls.lines().filter(|s| *s == "status").count(), 1);
    assert!(!calls.lines().any(|s| s == "start" || s == "validate"));
}
#[tokio::test]
async fn detached_single_attempt_survives_viewer_drop_before_during_and_after_admission() {
    for mode in ["validate_wait", "before", "after", "lost", "reject"] {
        let (_tmp, client, journal, request) = setup(mode);
        journal.accept(&request).unwrap();
        let id = request["request_id"].as_str().unwrap();
        // Terminate the submitting viewer process, not just its Child handle.
        // It runs the same shared launch helper as the interactive popup.
        let parent = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "submission_viewer_process"])
            .env("FIXTURE_VIEWER_ROOT", &client.root)
            .env("FIXTURE_VIEWER_REQUEST", id)
            .status()
            .unwrap();
        assert!(parent.success());
        let pid: u32 = fs::read_to_string(client.root.join("worker-pid"))
            .unwrap()
            .parse()
            .unwrap();
        if ["validate_wait", "before", "after"].contains(&mode) {
            wait(&client.root.join("waiting")).await;
            assert_eq!(unsafe { libc::getpgid(pid as i32) }, pid as i32);
            fs::write(client.root.join("release"), "").unwrap();
        }
        let end = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let entry: Value = serde_json::from_slice(
                &fs::read(journal.directory.join(format!("{id}.json"))).unwrap(),
            )
            .unwrap();
            if entry["receipt"]["phase"] != "pending_local" {
                break;
            }
            assert!(std::time::Instant::now() < end);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let rows = journal.list(&client, "launch").await.unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(
            rows[0]["phase"],
            if mode == "reject" {
                "failed"
            } else {
                "admitted"
            }
        );
        // Reopened worker cannot start again, even after the first process exited.
        assert!(journal.submit(&client, id).await.is_err());
        let calls = fs::read_to_string(client.root.join("calls")).unwrap();
        assert_eq!(
            calls.lines().filter(|s| *s == "start").count(),
            usize::from(mode != "reject")
        );
    }
}

#[test]
fn submission_viewer_process() {
    let Some(root) = std::env::var_os("FIXTURE_VIEWER_ROOT") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let client = Client {
        binary: root.join("backend"),
        root: root.clone(),
        herdr: "unused".into(),
        socket: "unused".into(),
        session: "fixture".into(),
    };
    let journal = Journal {
        directory: root.join("ui-intents"),
        root: root.clone(),
    };
    let child = journal
        .spawn_worker(
            std::path::Path::new(env!("CARGO_BIN_EXE_sessionsation-ui")),
            &client,
            &std::env::var("FIXTURE_VIEWER_REQUEST").unwrap(),
        )
        .unwrap();
    fs::write(root.join("worker-pid"), child.id().to_string()).unwrap();
    // No cooperative wait or async-runtime shutdown is required for admission.
    std::process::exit(0);
}
