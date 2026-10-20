use crate::command::{Arg, Command, CommandError, Invocation, Report};
use crate::desk::{require_ready, restrict_to_owner};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const MAX_FILE: usize = 1_000_000;
static SEQ: AtomicU64 = AtomicU64::new(1);

fn uid(prefix: &str) -> String {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{n}")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FileNode {
    id: String,
    #[serde(rename = "parentId")]
    parent_id: Option<String>,
    kind: String,
    name: String,
    #[serde(default)]
    binary: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Person {
    id: String,
    name: String,
    muted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Member {
    id: String,
    name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Message {
    id: String,
    from: String,
    text: String,
    reply: String,
    #[serde(rename = "workId", default)]
    work_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Agent {
    id: String,
    name: String,
    #[serde(rename = "folderIds")]
    folder_ids: Vec<String>,
    running: bool,
    #[serde(rename = "telegramId")]
    telegram_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Work {
    id: String,
    title: String,
    body: String,
    saved: String,
    history: Vec<String>,
    status: String,
    seen: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Model {
    provider: String,
    key: String,
    calls: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Reach {
    kind: String,
    value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Store {
    files: Vec<FileNode>,
    people: Vec<Person>,
    members: Vec<Member>,
    messages: Vec<Message>,
    agents: Vec<Agent>,
    works: Vec<Work>,
    model: Option<Model>,
    reach: Option<Reach>,
    #[serde(rename = "otherComputer", default)]
    other_computer: String,
    #[serde(default = "chinese")]
    language: String,
}

fn chinese() -> String {
    "中文".to_string()
}

fn store_path() -> PathBuf {
    crate::desk::state_dir().join("workspace.json")
}

fn blob_path(id: &str) -> PathBuf {
    crate::desk::state_dir().join("blobs").join(id)
}

pub fn wipe() -> Result<(), ()> {
    match fs::remove_file(store_path()) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(()),
    }
    match fs::remove_dir_all(crate::desk::state_dir().join("blobs")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(()),
    }
}

fn fresh(owner: &str) -> Store {
    Store {
        files: vec![FileNode {
            id: "root".to_string(),
            parent_id: None,
            kind: "folder".to_string(),
            name: "我的文件".to_string(),
            binary: false,
        }],
        people: Vec::new(),
        members: vec![Member {
            id: "owner".to_string(),
            name: owner.to_string(),
        }],
        messages: Vec::new(),
        agents: Vec::new(),
        works: Vec::new(),
        model: None,
        reach: None,
        other_computer: String::new(),
        language: chinese(),
    }
}

fn ensure(owner: &str) -> Result<Store, ()> {
    let path = store_path();
    if !path.exists() {
        let store = fresh(owner);
        save(&store)?;
        return Ok(store);
    }
    let text = fs::read_to_string(path).map_err(|_| ())?;
    let mut store: Store = serde_json::from_str(&text).map_err(|_| ())?;
    if !store.files.iter().any(|file| file.id == "root") {
        store.files.insert(
            0,
            FileNode {
                id: "root".to_string(),
                parent_id: None,
                kind: "folder".to_string(),
                name: "我的文件".to_string(),
                binary: false,
            },
        );
    }
    if let Some(member) = store.members.iter_mut().find(|member| member.id == "owner") {
        member.name = owner.to_string();
    } else {
        store.members.insert(
            0,
            Member {
                id: "owner".to_string(),
                name: owner.to_string(),
            },
        );
    }
    Ok(store)
}

fn save(store: &Store) -> Result<(), ()> {
    let dir = crate::desk::state_dir();
    fs::create_dir_all(&dir).map_err(|_| ())?;
    let path = store_path();
    let temporary = dir.join(".workspace.json.tmp");
    let text = serde_json::to_string_pretty(store).map_err(|_| ())?;
    fs::write(&temporary, text).map_err(|_| ())?;
    restrict_to_owner(&temporary)?;
    fs::rename(&temporary, &path).map_err(|_| ())
}

fn write_blob(id: &str, bytes: &[u8]) -> Result<(), ()> {
    let path = blob_path(id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| ())?;
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes).map_err(|_| ())?;
    restrict_to_owner(&temporary)?;
    fs::rename(&temporary, &path).map_err(|_| ())
}

fn file_text(node: &FileNode) -> String {
    if node.kind != "file" || node.binary {
        return String::new();
    }
    let bytes = fs::read(blob_path(&node.id)).unwrap_or_default();
    String::from_utf8(bytes).unwrap_or_default()
}

fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 4 {
        return "••••".to_string();
    }
    format!(
        "••••{}",
        chars[chars.len() - 4..].iter().collect::<String>()
    )
}

fn snapshot(store: &Store) -> Value {
    let files: Vec<Value> = store
        .files
        .iter()
        .map(|file| {
            json!({
                "id": file.id,
                "parentId": file.parent_id,
                "kind": file.kind,
                "name": file.name,
                "text": file_text(file),
                "binary": file.binary,
            })
        })
        .collect();
    let model = store.model.as_ref().map(|model| {
        json!({
            "provider": model.provider,
            "masked": mask_key(&model.key),
            "calls": model.calls,
        })
    });
    json!({
        "files": files,
        "people": store.people,
        "members": store.members,
        "messages": store.messages,
        "agents": store.agents,
        "works": store.works,
        "model": model,
        "reach": store.reach,
        "otherComputer": store.other_computer,
        "language": store.language,
    })
}

fn publish(
    store: &Store,
    summary: impl Into<String>,
    notice: &str,
    place: Option<&str>,
    open_work_id: Option<&str>,
    open_file_id: Option<&str>,
) -> Report {
    let mut data = Map::new();
    data.insert("workspace".to_string(), snapshot(store));
    data.insert("notice".to_string(), Value::String(notice.to_string()));
    if let Some(place) = place {
        data.insert("place".to_string(), Value::String(place.to_string()));
    }
    if let Some(id) = open_work_id {
        data.insert("openWorkId".to_string(), Value::String(id.to_string()));
    }
    if let Some(id) = open_file_id {
        data.insert("openFileId".to_string(), Value::String(id.to_string()));
    }
    Report::ok(summary).with_data(data)
}

fn load_mut() -> Result<(String, Store), Report> {
    let owner = require_ready()?;
    match ensure(&owner) {
        Ok(store) => Ok((owner, store)),
        Err(()) => Err(Report::fail("工作区暂时打不开。")),
    }
}

fn commit(store: &Store, summary: &str, notice: &str) -> Report {
    if save(store).is_err() {
        return Report::fail("工作区暂时打不开。");
    }
    publish(store, summary, notice, None, None, None)
}

fn same_name(store: &Store, parent: &str, name: &str, except: Option<&str>) -> bool {
    store.files.iter().any(|file| {
        file.parent_id.as_deref() == Some(parent)
            && file.name == name
            && Some(file.id.as_str()) != except
    })
}

fn folder_exists(store: &Store, id: &str) -> bool {
    store
        .files
        .iter()
        .any(|file| file.id == id && file.kind == "folder")
}

fn descendant_ids(files: &[FileNode], id: &str) -> BTreeSet<String> {
    let mut drop = BTreeSet::from([id.to_string()]);
    loop {
        let before = drop.len();
        for file in files {
            if let Some(parent) = &file.parent_id {
                if drop.contains(parent) {
                    drop.insert(file.id.clone());
                }
            }
        }
        if drop.len() == before {
            break;
        }
    }
    drop
}

pub fn status_report() -> Report {
    match load_mut() {
        Ok((_, store)) => publish(&store, "工作区已打开。", "", None, None, None),
        Err(report) => report,
    }
}

pub fn file_mkdir(name: &str, parent: &str) -> Report {
    let name = name.trim();
    let parent = if parent.is_empty() { "root" } else { parent };
    if name.is_empty() {
        return Report::fail("先写下名字。");
    }
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    if !folder_exists(&store, parent) {
        return Report::fail("这里没有这个文件夹。");
    }
    if same_name(&store, parent, name, None) {
        return Report::fail("这里已经有同名的东西。");
    }
    store.files.push(FileNode {
        id: uid("folder"),
        parent_id: Some(parent.to_string()),
        kind: "folder".to_string(),
        name: name.to_string(),
        binary: false,
    });
    commit(&store, "文件夹已留下。", "")
}

pub fn file_add(name: &str, parent: &str, text: Option<&str>, bytes: Option<&str>) -> Report {
    let name = name.trim();
    let parent = if parent.is_empty() { "root" } else { parent };
    if name.is_empty() {
        return Report::fail("先写下名字。");
    }
    let (binary, body) = if let Some(encoded) = bytes {
        match base64::engine::general_purpose::STANDARD.decode(encoded.trim()) {
            Ok(body) => (true, body),
            Err(_) => return Report::fail("这个文件放不进来。"),
        }
    } else {
        (false, text.unwrap_or("").as_bytes().to_vec())
    };
    if body.len() > MAX_FILE {
        return Report::fail("这个文件太大，先放一份更小的。");
    }
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    if !folder_exists(&store, parent) {
        return Report::fail("这里没有这个文件夹。");
    }
    if same_name(&store, parent, name, None) {
        return Report::fail("这里已经有同名的东西。");
    }
    let id = uid("file");
    if write_blob(&id, &body).is_err() {
        return Report::fail("工作区暂时打不开。");
    }
    store.files.push(FileNode {
        id: id.clone(),
        parent_id: Some(parent.to_string()),
        kind: "file".to_string(),
        name: name.to_string(),
        binary,
    });
    if save(&store).is_err() {
        return Report::fail("工作区暂时打不开。");
    }
    publish(&store, "文件已留下。", "", None, None, Some(&id))
}

pub fn file_rename(id: &str, name: &str) -> Report {
    let name = name.trim();
    if name.is_empty() {
        return Report::fail("先写下名字。");
    }
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    let Some(current) = store.files.iter().find(|file| file.id == id).cloned() else {
        return Report::fail("这里没有这个文件。");
    };
    let parent = current.parent_id.clone().unwrap_or_default();
    if same_name(&store, &parent, name, Some(id)) {
        return Report::fail("这里已经有同名的东西。");
    }
    if let Some(file) = store.files.iter_mut().find(|file| file.id == id) {
        file.name = name.to_string();
    }
    commit(&store, "名字已留下。", "")
}

pub fn file_move(id: &str, parent: &str) -> Report {
    if id == "root" || id == parent {
        return status_report();
    }
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    if !folder_exists(&store, parent) {
        return Report::fail("这里没有这个文件夹。");
    }
    let drop = descendant_ids(&store.files, id);
    if drop.contains(parent) {
        return Report::fail("不能移进自己里面。");
    }
    let Some(current) = store.files.iter().find(|file| file.id == id).cloned() else {
        return Report::fail("这里没有这个文件。");
    };
    if same_name(&store, parent, &current.name, Some(id)) {
        return Report::fail("那里已经有同名的东西。");
    }
    if let Some(file) = store.files.iter_mut().find(|file| file.id == id) {
        file.parent_id = Some(parent.to_string());
    }
    commit(&store, "已经移过去了。", "")
}

pub fn file_remove(id: &str) -> Report {
    if id == "root" {
        return Report::fail("这个文件夹要留着。");
    }
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    let drop = descendant_ids(&store.files, id);
    for id in &drop {
        if store
            .files
            .iter()
            .any(|file| file.id == *id && file.kind == "file")
        {
            let _ = fs::remove_file(blob_path(id));
        }
    }
    store.files.retain(|file| !drop.contains(&file.id));
    commit(&store, "已经删掉。", "")
}

pub fn work_create(title: &str) -> Report {
    let title = title.trim();
    if title.is_empty() {
        return Report::fail("先写下标题。");
    }
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    let id = uid("work");
    store.works.insert(
        0,
        Work {
            id: id.clone(),
            title: title.to_string(),
            body: String::new(),
            saved: String::new(),
            history: Vec::new(),
            status: "done".to_string(),
            seen: true,
        },
    );
    if save(&store).is_err() {
        return Report::fail("工作区暂时打不开。");
    }
    publish(&store, "工作已留下。", "", Some("work"), Some(&id), None)
}

pub fn work_keep(id: &str, body: &str) -> Report {
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    let Some(work) = store.works.iter_mut().find(|work| work.id == id) else {
        return Report::fail("这里没有这份工作。");
    };
    if work.body != body || work.saved != body {
        if work.saved != body {
            work.history.push(work.saved.clone());
            work.history = work
                .history
                .split_off(work.history.len().saturating_sub(30));
        }
        work.body = body.to_string();
        work.saved = body.to_string();
    }
    commit(&store, "修改已留下。", "")
}

pub fn work_undo(id: &str) -> Report {
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    let Some(work) = store.works.iter_mut().find(|work| work.id == id) else {
        return Report::fail("这里没有这份工作。");
    };
    if work.body != work.saved {
        work.body = work.saved.clone();
    } else if let Some(previous) = work.history.pop() {
        work.body = previous.clone();
        work.saved = previous;
    }
    commit(&store, "已撤销。", "")
}

pub fn work_dismiss(id: &str) -> Report {
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    if let Some(work) = store.works.iter_mut().find(|work| work.id == id) {
        work.seen = true;
    }
    commit(&store, "知道了。", "")
}

pub fn agent_create(name: &str, folders: &str, telegram: &str) -> Report {
    let name = name.trim();
    let telegram = telegram.trim();
    if name.is_empty() {
        return Report::fail("先写下 Agent 的名字。");
    }
    if !telegram.is_empty() && !telegram.chars().all(|ch| ch.is_ascii_digit()) {
        return Report::fail("Telegram 用户 ID 只能包含数字。");
    }
    let folder_ids: Vec<String> = folders
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect();
    if folder_ids.is_empty() {
        return Report::fail("勾上它能看的文件夹。");
    }
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    if folder_ids.iter().any(|id| !folder_exists(&store, id)) {
        return Report::fail("勾上它能看的文件夹。");
    }
    store.agents.push(Agent {
        id: uid("agent"),
        name: name.to_string(),
        folder_ids,
        running: true,
        telegram_id: telegram.to_string(),
    });
    commit(&store, "Agent 已留下。", "")
}

pub fn agent_toggle(id: &str) -> Report {
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    if let Some(agent) = store.agents.iter_mut().find(|agent| agent.id == id) {
        agent.running = !agent.running;
    }
    commit(&store, "Agent 已更新。", "")
}

pub fn agent_telegram(id: &str, telegram: &str) -> Report {
    let telegram = telegram.trim();
    if !telegram.is_empty() && !telegram.chars().all(|ch| ch.is_ascii_digit()) {
        return Report::fail("Telegram 用户 ID 只能包含数字。");
    }
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    if let Some(agent) = store.agents.iter_mut().find(|agent| agent.id == id) {
        agent.telegram_id = telegram.to_string();
    }
    commit(&store, "已记下。", "")
}

pub fn person_add(name: &str) -> Report {
    let name = name.trim();
    if name.is_empty() {
        return Report::fail("先写下名字。");
    }
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    store.people.push(Person {
        id: uid("person"),
        name: name.to_string(),
        muted: false,
    });
    commit(&store, "已认识。", "")
}

pub fn person_mute(id: &str) -> Report {
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    if let Some(person) = store.people.iter_mut().find(|person| person.id == id) {
        person.muted = !person.muted;
    }
    commit(&store, "已更新。", "")
}

pub fn message_reply(id: &str, text: &str) -> Report {
    let text = text.trim();
    if text.is_empty() {
        return Report::fail("先写下答复。");
    }
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    let Some(message) = store.messages.iter_mut().find(|message| message.id == id) else {
        return Report::fail("这里没有这条消息。");
    };
    message.reply = text.to_string();
    commit(&store, "已答复。", "")
}

pub fn model_connect(provider: &str, key: &str) -> Report {
    let provider = provider.trim();
    let key = key.trim();
    if provider.is_empty() || key.is_empty() {
        return Report::fail("写下服务的名字和密钥。");
    }
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    let calls = store.model.as_ref().map(|model| model.calls).unwrap_or(0);
    store.model = Some(Model {
        provider: provider.to_string(),
        key: key.to_string(),
        calls,
    });
    commit(&store, "模型已接上。", "")
}

pub fn model_disconnect() -> Report {
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    store.model = None;
    commit(&store, "模型已断开。", "")
}

pub fn member_add(name: &str) -> Report {
    let name = name.trim();
    if name.is_empty() {
        return Report::fail("先写下名字。");
    }
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    store.members.push(Member {
        id: uid("member"),
        name: name.to_string(),
    });
    commit(&store, "已加上。", "")
}

pub fn reach(kind: &str, value: &str) -> Report {
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    let owner = store
        .members
        .iter()
        .find(|member| member.id == "owner")
        .map(|member| member.name.clone())
        .unwrap_or_default();
    if kind == "local" {
        store.reach = Some(Reach {
            kind: "local".to_string(),
            value: owner,
        });
        return commit(&store, "已记下名字。", "");
    }
    let domain = value.trim();
    if domain.is_empty() || !domain.contains('.') {
        return Report::fail("域名格式不正确");
    }
    store.reach = Some(Reach {
        kind: "domain".to_string(),
        value: domain.to_string(),
    });
    commit(&store, "已记下域名。", "")
}

pub fn note(address: &str) -> Report {
    let address = address.trim();
    if address.is_empty() {
        return Report::fail("先写下那台电脑的地址。");
    }
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    store.other_computer = address.to_string();
    commit(
        &store,
        "已记下。",
        "已记下。那台电脑准备好之后，在这里继续。",
    )
}

pub fn language(name: &str) -> Report {
    let name = if name.trim().is_empty() {
        "中文"
    } else {
        name.trim()
    };
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    store.language = name.to_string();
    let notice = if name == "中文" {
        ""
    } else {
        "语言已记下。这一版页面仍是中文。"
    };
    commit(&store, "语言已记下。", notice)
}

enum Prepared {
    Done(Report),
    Call {
        provider: String,
        key: String,
        name: String,
        text: String,
    },
}

fn redirect(store: &Store, place: &str, notice: &str) -> Report {
    publish(store, notice, notice, Some(place), None, None)
}

fn prepare_summary(file_id: &str) -> Prepared {
    let (_, store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return Prepared::Done(report),
    };
    let Some(file) = store
        .files
        .iter()
        .find(|file| file.id == file_id && file.kind == "file")
        .cloned()
    else {
        return Prepared::Done(Report::fail("先选一个文件。"));
    };
    if store.agents.is_empty() {
        return Prepared::Done(redirect(&store, "agent", "先做一个 Agent。"));
    }
    let Some(agent) = store.agents.iter().find(|agent| agent.running) else {
        return Prepared::Done(redirect(&store, "agent", "Agent 停着。先让它开始。"));
    };
    let parent = file.parent_id.clone().unwrap_or_default();
    let allowed = agent
        .folder_ids
        .iter()
        .any(|id| id == &parent || id == "root");
    if !allowed {
        return Prepared::Done(redirect(
            &store,
            "agent",
            "这个 Agent 还不能看放文件的那个文件夹。",
        ));
    }
    let Some(model) = store.model.clone() else {
        return Prepared::Done(redirect(
            &store,
            "model",
            "先接上一个模型。接上之前也可以回到今天。",
        ));
    };
    let text = file_text(&file);
    if text.trim().is_empty() {
        return Prepared::Done(write_summary(
            &file.name,
            Ok(format!("《{}》已经放进工作。文件里还没有正文。", file.name)),
            false,
        ));
    }
    Prepared::Call {
        provider: model.provider,
        key: model.key,
        name: file.name,
        text,
    }
}

pub fn write_summary(file_name: &str, result: Result<String, String>, count_call: bool) -> Report {
    let (_, mut store) = match load_mut() {
        Ok(pair) => pair,
        Err(report) => return report,
    };
    let (body, status) = match result {
        Ok(body) => (body, "done".to_string()),
        Err(body) => (body, "failed".to_string()),
    };
    let id = uid("work");
    if status == "done" {
        store.messages.insert(
            0,
            Message {
                id: uid("msg"),
                from: "工作区".to_string(),
                text: format!("《{file_name}》的摘要放在工作里了。"),
                reply: String::new(),
                work_id: Some(id.clone()),
            },
        );
        if count_call {
            if let Some(model) = store.model.as_mut() {
                model.calls = model.calls.saturating_add(1);
            }
        }
    }
    store.works.insert(
        0,
        Work {
            id: id.clone(),
            title: format!("{file_name} 的摘要"),
            body: body.clone(),
            saved: body,
            history: Vec::new(),
            status,
            seen: false,
        },
    );
    if save(&store).is_err() {
        return Report::fail("工作区暂时打不开。");
    }
    publish(
        &store,
        "摘要已放进工作。",
        "",
        Some("work"),
        Some(&id),
        None,
    )
}

pub fn call_model(
    provider: &str,
    key: &str,
    file_name: &str,
    text: &str,
) -> Result<String, String> {
    let Some(url) = chat_url(provider) else {
        return Err("模型服务需要一个 http 地址，摘要没有发出去。".to_string());
    };
    let clip: String = text.chars().take(4000).collect();
    let payload = json!({
        "model": "gpt-4o-mini",
        "messages": [{
            "role": "user",
            "content": format!("用中文写一段简短摘要。文件名：{file_name}\n\n{clip}")
        }]
    });
    let response = ureq::post(&url)
        .timeout(Duration::from_secs(20))
        .set("Authorization", &format!("Bearer {key}"))
        .send_json(payload);
    let payload = match response {
        Ok(response) => response.into_json().unwrap_or(Value::Null),
        Err(ureq::Error::Status(_, response)) => {
            let payload: Value = response.into_json().unwrap_or(Value::Null);
            let message = payload["error"]["message"]
                .as_str()
                .unwrap_or("模型没有返回摘要。");
            return Err(message.to_string());
        }
        Err(_) => return Err("模型服务没有连上。".to_string()),
    };
    let content = payload["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    if content.is_empty() {
        return Err("模型没有返回摘要。".to_string());
    }
    Ok(content)
}

fn chat_url(provider: &str) -> Option<String> {
    let url = provider.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    if url.contains("chat/completions") {
        return Some(url.to_string());
    }
    Some(format!("{}/chat/completions", url.trim_end_matches('/')))
}

const PARENT: Arg = Arg {
    name: "parent",
    takes_value: true,
    summary: "所在文件夹",
};
const TEXT: Arg = Arg {
    name: "text",
    takes_value: true,
    summary: "正文",
};
const BYTES: Arg = Arg {
    name: "bytes",
    takes_value: true,
    summary: "文件内容的 base64",
};
const ID: Arg = Arg {
    name: "id",
    takes_value: true,
    summary: "编号",
};
const NAME: Arg = Arg {
    name: "name",
    takes_value: true,
    summary: "名字",
};
const TITLE: Arg = Arg {
    name: "title",
    takes_value: true,
    summary: "标题",
};
const BODY: Arg = Arg {
    name: "body",
    takes_value: true,
    summary: "正文",
};
const FOLDERS: Arg = Arg {
    name: "folders",
    takes_value: true,
    summary: "能看的文件夹，用逗号分开",
};
const TELEGRAM: Arg = Arg {
    name: "telegram",
    takes_value: true,
    summary: "Telegram 用户 ID",
};
const PROVIDER: Arg = Arg {
    name: "provider",
    takes_value: true,
    summary: "模型服务的地址",
};
const KEY: Arg = Arg {
    name: "key",
    takes_value: true,
    summary: "密钥",
};
const KIND: Arg = Arg {
    name: "kind",
    takes_value: true,
    summary: "local 或 domain",
};
const VALUE: Arg = Arg {
    name: "value",
    takes_value: true,
    summary: "域名",
};
const ADDRESS: Arg = Arg {
    name: "address",
    takes_value: true,
    summary: "另一台电脑的地址",
};
const FILE: Arg = Arg {
    name: "file",
    takes_value: true,
    summary: "文件编号",
};

pub static WORKSPACE: Command = Command {
    name: "workspace",
    summary: "这台电脑上的文件、消息、Agent 和工作",
    surface: crate::command::Surface::Both,
    args: &[],
    children: &[
        STATUS, FILE_CMD, WORK, AGENT, PERSON, MESSAGE, MODEL, SUMMARY, MEMBER, REACH, NOTE,
        LANGUAGE,
    ],
    run: None,
};

static STATUS: Command = Command {
    name: "status",
    summary: "读出工作区里记下的东西",
    surface: crate::command::Surface::Both,
    args: &[],
    children: &[],
    run: Some(run_status),
};

static FILE_CMD: Command = Command {
    name: "file",
    summary: "文件和文件夹",
    surface: crate::command::Surface::Both,
    args: &[],
    children: &[FILE_MKDIR, FILE_ADD, FILE_RENAME, FILE_MOVE, FILE_REMOVE],
    run: None,
};

static FILE_MKDIR: Command = Command {
    name: "mkdir",
    summary: "新建文件夹",
    surface: crate::command::Surface::Both,
    args: &[NAME, PARENT],
    children: &[],
    run: Some(run_mkdir),
};

static FILE_ADD: Command = Command {
    name: "add",
    summary: "放入一份文件",
    surface: crate::command::Surface::Both,
    args: &[NAME, PARENT, TEXT, BYTES],
    children: &[],
    run: Some(run_add),
};

static FILE_RENAME: Command = Command {
    name: "rename",
    summary: "改名",
    surface: crate::command::Surface::Both,
    args: &[ID, NAME],
    children: &[],
    run: Some(run_rename),
};

static FILE_MOVE: Command = Command {
    name: "move",
    summary: "移动",
    surface: crate::command::Surface::Both,
    args: &[ID, PARENT],
    children: &[],
    run: Some(run_move),
};

static FILE_REMOVE: Command = Command {
    name: "remove",
    summary: "删除",
    surface: crate::command::Surface::Both,
    args: &[ID],
    children: &[],
    run: Some(run_remove),
};

static WORK: Command = Command {
    name: "work",
    summary: "可撤销的工作",
    surface: crate::command::Surface::Both,
    args: &[],
    children: &[WORK_CREATE, WORK_KEEP, WORK_UNDO, WORK_DISMISS],
    run: None,
};

static WORK_CREATE: Command = Command {
    name: "create",
    summary: "新的一份工作",
    surface: crate::command::Surface::Both,
    args: &[TITLE],
    children: &[],
    run: Some(run_work_create),
};

static WORK_KEEP: Command = Command {
    name: "keep",
    summary: "留下当前修改",
    surface: crate::command::Surface::Both,
    args: &[ID, BODY],
    children: &[],
    run: Some(run_work_keep),
};

static WORK_UNDO: Command = Command {
    name: "undo",
    summary: "撤销最近一次留下的修改",
    surface: crate::command::Surface::Both,
    args: &[ID],
    children: &[],
    run: Some(run_work_undo),
};

static WORK_DISMISS: Command = Command {
    name: "dismiss",
    summary: "今天里这条已经看过",
    surface: crate::command::Surface::Both,
    args: &[ID],
    children: &[],
    run: Some(run_work_dismiss),
};

static AGENT: Command = Command {
    name: "agent",
    summary: "助手",
    surface: crate::command::Surface::Both,
    args: &[],
    children: &[AGENT_CREATE, AGENT_TOGGLE, AGENT_TELEGRAM],
    run: None,
};

static AGENT_CREATE: Command = Command {
    name: "create",
    summary: "做一个 Agent",
    surface: crate::command::Surface::Both,
    args: &[NAME, FOLDERS, TELEGRAM],
    children: &[],
    run: Some(run_agent_create),
};

static AGENT_TOGGLE: Command = Command {
    name: "toggle",
    summary: "让 Agent 开始或停下",
    surface: crate::command::Surface::Both,
    args: &[ID],
    children: &[],
    run: Some(run_agent_toggle),
};

static AGENT_TELEGRAM: Command = Command {
    name: "telegram",
    summary: "记下 Telegram 用户 ID",
    surface: crate::command::Surface::Both,
    args: &[ID, TELEGRAM],
    children: &[],
    run: Some(run_agent_telegram),
};

static PERSON: Command = Command {
    name: "person",
    summary: "我认识的人",
    surface: crate::command::Surface::Both,
    args: &[],
    children: &[PERSON_ADD, PERSON_MUTE],
    run: None,
};

static PERSON_ADD: Command = Command {
    name: "add",
    summary: "认识一个人",
    surface: crate::command::Surface::Both,
    args: &[NAME],
    children: &[],
    run: Some(run_person_add),
};

static PERSON_MUTE: Command = Command {
    name: "mute",
    summary: "静音或取消静音",
    surface: crate::command::Surface::Both,
    args: &[ID],
    children: &[],
    run: Some(run_person_mute),
};

static MESSAGE: Command = Command {
    name: "message",
    summary: "要答复的消息",
    surface: crate::command::Surface::Both,
    args: &[],
    children: &[MESSAGE_REPLY],
    run: None,
};

static MESSAGE_REPLY: Command = Command {
    name: "reply",
    summary: "答复一条消息",
    surface: crate::command::Surface::Both,
    args: &[ID, TEXT],
    children: &[],
    run: Some(run_message_reply),
};

static MODEL: Command = Command {
    name: "model",
    summary: "模型服务",
    surface: crate::command::Surface::Both,
    args: &[],
    children: &[MODEL_CONNECT, MODEL_DISCONNECT],
    run: None,
};

static MODEL_CONNECT: Command = Command {
    name: "connect",
    summary: "接上模型服务",
    surface: crate::command::Surface::Both,
    args: &[PROVIDER, KEY],
    children: &[],
    run: Some(run_model_connect),
};

static MODEL_DISCONNECT: Command = Command {
    name: "disconnect",
    summary: "断开模型服务",
    surface: crate::command::Surface::Both,
    args: &[],
    children: &[],
    run: Some(run_model_disconnect),
};

static SUMMARY: Command = Command {
    name: "summary",
    summary: "把一份文件做成摘要，放到工作里",
    surface: crate::command::Surface::Both,
    args: &[FILE],
    children: &[],
    run: Some(run_summary),
};

static MEMBER: Command = Command {
    name: "member",
    summary: "能进入这台电脑的人",
    surface: crate::command::Surface::Both,
    args: &[],
    children: &[MEMBER_ADD],
    run: None,
};

static MEMBER_ADD: Command = Command {
    name: "add",
    summary: "再加一个人",
    surface: crate::command::Surface::Both,
    args: &[NAME],
    children: &[],
    run: Some(run_member_add),
};

static REACH: Command = Command {
    name: "reach",
    summary: "记下别人以后怎么找到这台电脑",
    surface: crate::command::Surface::Both,
    args: &[KIND, VALUE],
    children: &[],
    run: Some(run_reach),
};

static NOTE: Command = Command {
    name: "note",
    summary: "记下另一台正在等待的电脑",
    surface: crate::command::Surface::Both,
    args: &[ADDRESS],
    children: &[],
    run: Some(run_note),
};

static LANGUAGE: Command = Command {
    name: "language",
    summary: "记下语言",
    surface: crate::command::Surface::Both,
    args: &[NAME],
    children: &[],
    run: Some(run_language),
};

fn run_status(_: &Invocation) -> Result<Report, CommandError> {
    Ok(crate::desk::with_desk(status_report))
}

fn run_mkdir(invocation: &Invocation) -> Result<Report, CommandError> {
    let name = invocation.value("name").unwrap_or("").to_string();
    let parent = invocation.value("parent").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| file_mkdir(&name, &parent)))
}

fn run_add(invocation: &Invocation) -> Result<Report, CommandError> {
    let name = invocation.value("name").unwrap_or("").to_string();
    let parent = invocation.value("parent").unwrap_or("").to_string();
    let text = invocation.value("text").map(str::to_string);
    let bytes = invocation.value("bytes").map(str::to_string);
    Ok(crate::desk::with_desk(|| {
        file_add(&name, &parent, text.as_deref(), bytes.as_deref())
    }))
}

fn run_rename(invocation: &Invocation) -> Result<Report, CommandError> {
    let id = invocation.value("id").unwrap_or("").to_string();
    let name = invocation.value("name").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| file_rename(&id, &name)))
}

fn run_move(invocation: &Invocation) -> Result<Report, CommandError> {
    let id = invocation.value("id").unwrap_or("").to_string();
    let parent = invocation.value("parent").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| file_move(&id, &parent)))
}

fn run_remove(invocation: &Invocation) -> Result<Report, CommandError> {
    let id = invocation.value("id").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| file_remove(&id)))
}

fn run_work_create(invocation: &Invocation) -> Result<Report, CommandError> {
    let title = invocation.value("title").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| work_create(&title)))
}

fn run_work_keep(invocation: &Invocation) -> Result<Report, CommandError> {
    let id = invocation.value("id").unwrap_or("").to_string();
    let body = invocation.value("body").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| work_keep(&id, &body)))
}

fn run_work_undo(invocation: &Invocation) -> Result<Report, CommandError> {
    let id = invocation.value("id").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| work_undo(&id)))
}

fn run_work_dismiss(invocation: &Invocation) -> Result<Report, CommandError> {
    let id = invocation.value("id").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| work_dismiss(&id)))
}

fn run_agent_create(invocation: &Invocation) -> Result<Report, CommandError> {
    let name = invocation.value("name").unwrap_or("").to_string();
    let folders = invocation.value("folders").unwrap_or("").to_string();
    let telegram = invocation.value("telegram").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| {
        agent_create(&name, &folders, &telegram)
    }))
}

fn run_agent_toggle(invocation: &Invocation) -> Result<Report, CommandError> {
    let id = invocation.value("id").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| agent_toggle(&id)))
}

fn run_agent_telegram(invocation: &Invocation) -> Result<Report, CommandError> {
    let id = invocation.value("id").unwrap_or("").to_string();
    let telegram = invocation.value("telegram").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| agent_telegram(&id, &telegram)))
}

fn run_person_add(invocation: &Invocation) -> Result<Report, CommandError> {
    let name = invocation.value("name").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| person_add(&name)))
}

fn run_person_mute(invocation: &Invocation) -> Result<Report, CommandError> {
    let id = invocation.value("id").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| person_mute(&id)))
}

fn run_message_reply(invocation: &Invocation) -> Result<Report, CommandError> {
    let id = invocation.value("id").unwrap_or("").to_string();
    let text = invocation.value("text").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| message_reply(&id, &text)))
}

fn run_model_connect(invocation: &Invocation) -> Result<Report, CommandError> {
    let provider = invocation.value("provider").unwrap_or("").to_string();
    let key = invocation.value("key").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| model_connect(&provider, &key)))
}

fn run_model_disconnect(_: &Invocation) -> Result<Report, CommandError> {
    Ok(crate::desk::with_desk(model_disconnect))
}

fn run_summary(invocation: &Invocation) -> Result<Report, CommandError> {
    let file_id = invocation.value("file").unwrap_or("").to_string();
    let prepared = crate::desk::with_desk(|| prepare_summary(&file_id));
    match prepared {
        Prepared::Done(report) => Ok(report),
        Prepared::Call {
            provider,
            key,
            name,
            text,
        } => {
            let result = call_model(&provider, &key, &name, &text);
            let count = result.is_ok();
            Ok(crate::desk::with_desk(|| {
                write_summary(&name, result, count)
            }))
        }
    }
}

fn run_member_add(invocation: &Invocation) -> Result<Report, CommandError> {
    let name = invocation.value("name").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| member_add(&name)))
}

fn run_reach(invocation: &Invocation) -> Result<Report, CommandError> {
    let kind = invocation.value("kind").unwrap_or("").to_string();
    let value = invocation.value("value").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| reach(&kind, &value)))
}

fn run_note(invocation: &Invocation) -> Result<Report, CommandError> {
    let address = invocation.value("address").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| note(&address)))
}

fn run_language(invocation: &Invocation) -> Result<Report, CommandError> {
    let name = invocation.value("name").unwrap_or("").to_string();
    Ok(crate::desk::with_desk(|| language(&name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{dispatch, Surface};
    use crate::desk::{activate_report, finish_report, isolate};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn scratch() -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("buckydesk-ws-{}-{}", std::process::id(), nanos));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ready() {
        assert!(activate_report("安", "12345678", "文档/key").ok);
        assert!(finish_report().ok);
    }

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_string()).collect()
    }

    #[test]
    fn files_people_and_reset_live_on_disk() {
        let dir = scratch();
        isolate(&dir, "127.0.0.1:1", || {
            ready();
            let added = dispatch(
                &argv(&[
                    "workspace",
                    "file",
                    "add",
                    "--name",
                    "草稿",
                    "--parent",
                    "root",
                    "--text",
                    "你好",
                ]),
                Surface::Http,
            )
            .unwrap();
            assert!(added.ok);
            let files = added.data["workspace"]["files"].as_array().unwrap();
            let draft = files.iter().find(|file| file["name"] == "草稿").unwrap();
            assert_eq!(draft["text"], "你好");
            assert!(draft["id"].as_str().unwrap() != "root");
            let raw = fs::read_to_string(dir.join("workspace.json")).unwrap();
            assert!(raw.contains("草稿"));
            assert!(!raw.contains("12345678"));
            let blob = dir.join("blobs").join(draft["id"].as_str().unwrap());
            assert_eq!(fs::read_to_string(blob).unwrap(), "你好");

            let agent = dispatch(
                &argv(&[
                    "workspace",
                    "agent",
                    "create",
                    "--name",
                    "助手",
                    "--folders",
                    "root",
                ]),
                Surface::Http,
            )
            .unwrap();
            assert!(agent.ok);

            let blocked = dispatch(
                &argv(&["workspace", "summary", "--file", "missing"]),
                Surface::Http,
            )
            .unwrap();
            assert!(!blocked.ok);

            let needs_model = dispatch(
                &argv(&[
                    "workspace",
                    "summary",
                    "--file",
                    draft["id"].as_str().unwrap(),
                ]),
                Surface::Http,
            )
            .unwrap();
            assert!(needs_model.ok);
            assert_eq!(needs_model.data["place"], "model");

            let named = dispatch(
                &argv(&[
                    "workspace",
                    "model",
                    "connect",
                    "--provider",
                    "本地服务",
                    "--key",
                    "secret-key",
                ]),
                Surface::Http,
            )
            .unwrap();
            assert!(named.ok);
            assert!(named.data["workspace"]["model"]["key"].is_null());
            assert_eq!(named.data["workspace"]["model"]["masked"], "••••-key");
            let stored = fs::read_to_string(dir.join("workspace.json")).unwrap();
            assert!(stored.contains("secret-key"));

            let failed = dispatch(
                &argv(&[
                    "workspace",
                    "summary",
                    "--file",
                    draft["id"].as_str().unwrap(),
                ]),
                Surface::Http,
            )
            .unwrap();
            assert!(failed.ok);
            assert_eq!(failed.data["place"], "work");
            let work = &failed.data["workspace"]["works"][0];
            assert_eq!(work["status"], "failed");
            assert!(work["body"].as_str().unwrap().contains("http"));

            assert!(
                dispatch(&argv(&["identity", "reset"]), Surface::Http)
                    .unwrap()
                    .ok
            );
            assert!(!dir.join("workspace.json").exists());
            assert!(!dir.join("blobs").exists());
        });
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn summary_uses_the_model_service() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 8192];
            let _ = stream.read(&mut buf);
            let body = br#"{"choices":[{"message":{"content":"summary-from-model"}}]}"#;
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });
        let dir = scratch();
        isolate(&dir, "127.0.0.1:1", || {
            ready();
            let added = dispatch(
                &argv(&[
                    "workspace",
                    "file",
                    "add",
                    "--name",
                    "笔记",
                    "--parent",
                    "root",
                    "--text",
                    "正文",
                ]),
                Surface::Http,
            )
            .unwrap();
            let id = added.data["workspace"]["files"]
                .as_array()
                .unwrap()
                .iter()
                .find(|file| file["name"] == "笔记")
                .unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string();
            assert!(
                dispatch(
                    &argv(&[
                        "workspace",
                        "agent",
                        "create",
                        "--name",
                        "助手",
                        "--folders",
                        "root"
                    ]),
                    Surface::Http
                )
                .unwrap()
                .ok
            );
            let provider = format!("http://{address}/v1");
            assert!(
                dispatch(
                    &argv(&[
                        "workspace",
                        "model",
                        "connect",
                        "--provider",
                        &provider,
                        "--key",
                        "sk-test",
                    ]),
                    Surface::Http
                )
                .unwrap()
                .ok
            );
            let report = dispatch(
                &argv(&["workspace", "summary", "--file", &id]),
                Surface::Http,
            )
            .unwrap();
            assert!(report.ok, "{}", report.summary);
            let work = &report.data["workspace"]["works"][0];
            assert_eq!(work["status"], "done");
            assert_eq!(work["body"], "summary-from-model");
            assert_eq!(report.data["workspace"]["model"]["calls"], 1);
            assert_eq!(report.data["workspace"]["messages"][0]["from"], "工作区");
        });
        let _ = fs::remove_dir_all(dir);
    }
}
