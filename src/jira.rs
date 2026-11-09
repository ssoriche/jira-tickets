//! Jira Cloud client: REST v3 for issues, Agile 1.0 for rank and sprints.
//! Blocking; always called from background threads.

use crate::auth::Login;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub static READ_ONLY: AtomicBool = AtomicBool::new(false);

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub enum Error {
    Unauthorized(String),
    RateLimited(String),
    /// The request never got an HTTP response (DNS, connect, TLS, timeout).
    Network(String),
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unauthorized(m) | Error::RateLimited(m) | Error::Network(m) | Error::Other(m) => f.write_str(m),
        }
    }
}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        Error::Network(format!("Network error: {e}"))
    }
}

const READ_ONLY_MSG: &str = "read-only mode, so nothing was sent to Jira";

pub fn check_writable() -> Result<()> {
    if READ_ONLY.load(Ordering::Relaxed) {
        return Err(Error::Other(READ_ONLY_MSG.into()));
    }
    Ok(())
}

/// The error `check_writable` returns in read-only mode.
pub fn is_read_only(e: &Error) -> bool {
    matches!(e, Error::Other(m) if m == READ_ONLY_MSG)
}

pub fn map_status(code: u16, body: &str) -> Error {
    let msg = extract_error(body).unwrap_or_else(|| body.chars().take(300).collect());
    match code {
        // Only 401 signs the user out; a 403 is a permission problem on one call.
        401 => Error::Unauthorized(format!("authentication failed ({code}): {msg}")),
        429 => Error::RateLimited(format!("rate limited (429): {msg}")),
        _ => Error::Other(format!("Jira returned {code}: {msg}")),
    }
}

/// Jira error bodies: {"errorMessages": [...], "errors": {field: msg}}.
fn extract_error(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let messages = v["errorMessages"].as_array().map(Vec::as_slice).unwrap_or_default();
    let mut parts: Vec<String> = messages.iter().filter_map(|m| m.as_str().map(String::from)).collect();
    if let Some(errs) = v["errors"].as_object() {
        parts.extend(errs.iter().map(|(k, m)| format!("{k}: {}", m.as_str().unwrap_or(""))));
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

pub fn http_agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(60)))
        .user_agent("jira-tickets")
        .tls_config(tls)
        .build()
        .into()
}

pub struct Client {
    base: String,
    agent: ureq::Agent,
    auth: String,
}

// ---------- Models ----------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct User {
    pub account_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Status {
    pub name: String,
    /// statusCategory.key: "new", "indeterminate", or "done".
    pub category: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub created: String,
    pub body: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Issue {
    pub key: String,
    pub id: String,
    pub summary: String,
    pub issue_type: String,
    pub status: Status,
    pub priority: String,
    pub labels: Vec<String>,
    pub assignee: Option<User>,
    pub reporter: Option<User>,
    pub parent_key: Option<String>,
    pub parent_id: Option<String>,
    pub parent_summary: Option<String>,
    pub created: String,
    pub updated: String,
    pub due_date: Option<String>,
    pub status_changed: Option<String>,
    pub rank: String,
    pub story_points: Option<f64>,
    pub sprint: Option<String>,
    pub description: Option<Value>,
    pub comments: Vec<Comment>,
    /// Any customfield_* that was requested, raw.
    pub custom: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Transition {
    pub id: String,
    pub name: String,
    pub to: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Resolution {
    pub id: String,
    pub name: String,
}

/// An issue type a project lets the user create (`Client::creatable_issue_types`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IssueType {
    pub id: String,
    pub name: String,
    pub subtask: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sprint {
    pub id: u64,
    pub name: String,
    pub state: String,
}

pub const RANK_FIELD: &str = "customfield_10019";
pub const SPRINT_FIELD: &str = "customfield_10020";
pub const POINTS_FIELD: &str = "customfield_10016";

pub const LIST_FIELDS: &[&str] = &[
    "summary",
    "status",
    "assignee",
    "reporter",
    "parent",
    "labels",
    "priority",
    "issuetype",
    "created",
    "updated",
    "duedate",
    RANK_FIELD,
    "statuscategorychangedate",
    SPRINT_FIELD,
    POINTS_FIELD,
];

pub const DETAIL_FIELDS: &[&str] = &["description", "comment"];

fn str_at(v: &Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.as_str().map(String::from)
}

fn user_at(v: &Value) -> Option<User> {
    let display_name = str_at(v, &["displayName"])?;
    let account_id = str_at(v, &["accountId"]).unwrap_or_default();
    if let Some(url) = avatar_url_in(v)
        && !account_id.is_empty()
    {
        AVATAR_URLS.lock().unwrap_or_else(|e| e.into_inner()).insert(account_id.clone(), url);
    }
    Some(User { account_id, display_name })
}

/// Avatar URLs seen in any parsed user, by account id. A side table rather than a `User` field, so the many
/// `User { .. }` literals stay as they are. `crate::avatar` reads it.
static AVATAR_URLS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, String>>> = std::sync::LazyLock::new(Default::default);

/// The avatar URL last seen for `account_id`, if any parsed response carried one.
pub fn known_avatar_url(account_id: &str) -> Option<String> {
    AVATAR_URLS.lock().unwrap_or_else(|e| e.into_inner()).get(account_id).cloned()
}

/// The best avatar URL in a user object's `avatarUrls`: 48x48, else the largest smaller one.
fn avatar_url_in(user: &Value) -> Option<String> {
    ["48x48", "32x32", "24x24", "16x16"].iter().find_map(|k| user["avatarUrls"][k].as_str().filter(|u| !u.is_empty()).map(String::from))
}

/// Percent-encodes everything but RFC 3986 unreserved characters, for one query value.
fn urlencode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// True when `url` is on the same scheme and host as `base`, so it may get the Jira credentials.
/// Avatars on other hosts (Atlassian's avatar CDN, Gravatar) are public and never see the token.
fn same_origin(base: &str, url: &str) -> bool {
    let origin = |u: &str| -> Option<String> {
        let (scheme, rest) = u.split_once("://")?;
        let host = rest.split(['/', '?', '#']).next()?;
        Some(format!("{}://{}", scheme.to_ascii_lowercase(), host.to_ascii_lowercase()))
    };
    origin(base).is_some_and(|b| origin(url) == Some(b))
}

/// The issue's sprint as Jira's board sees it: the last one still `active` or `future`. An issue whose
/// sprints are all closed is back in the backlog, so it has none (matching `store::sprint_jql`'s Backlog).
fn open_sprint(sprints: &Value) -> Option<String> {
    sprints.as_array()?.iter().rfind(|s| matches!(s["state"].as_str(), Some("active" | "future"))).and_then(|s| s["name"].as_str().map(String::from))
}

pub fn parse_issue(v: &Value) -> Issue {
    let f = &v["fields"];
    let mut custom = BTreeMap::new();
    if let Some(obj) = f.as_object() {
        for (k, val) in obj {
            if k.starts_with("customfield_") && !val.is_null() {
                custom.insert(k.clone(), val.clone());
            }
        }
    }
    let sprint = open_sprint(&f[SPRINT_FIELD]);
    let comments = f["comment"]["comments"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|c| Comment {
                    id: str_at(c, &["id"]).unwrap_or_default(),
                    author: str_at(c, &["author", "displayName"]).unwrap_or_default(),
                    created: str_at(c, &["created"]).unwrap_or_default(),
                    body: c["body"].clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    Issue {
        key: str_at(v, &["key"]).unwrap_or_default(),
        id: str_at(v, &["id"]).unwrap_or_default(),
        summary: str_at(f, &["summary"]).unwrap_or_default(),
        issue_type: str_at(f, &["issuetype", "name"]).unwrap_or_default(),
        status: Status {
            name: str_at(f, &["status", "name"]).unwrap_or_default(),
            category: str_at(f, &["status", "statusCategory", "key"]).unwrap_or_default(),
        },
        priority: str_at(f, &["priority", "name"]).unwrap_or_default(),
        labels: f["labels"].as_array().map(|a| a.iter().filter_map(|l| l.as_str().map(String::from)).collect()).unwrap_or_default(),
        assignee: user_at(&f["assignee"]),
        reporter: user_at(&f["reporter"]),
        parent_key: str_at(f, &["parent", "key"]),
        parent_id: str_at(f, &["parent", "id"]),
        parent_summary: str_at(f, &["parent", "fields", "summary"]),
        created: str_at(f, &["created"]).unwrap_or_default(),
        updated: str_at(f, &["updated"]).unwrap_or_default(),
        due_date: str_at(f, &["duedate"]),
        status_changed: str_at(f, &["statuscategorychangedate"]),
        rank: str_at(f, &[RANK_FIELD]).unwrap_or_default(),
        story_points: f[POINTS_FIELD].as_f64(),
        sprint,
        description: f.get("description").filter(|d| !d.is_null()).cloned(),
        comments,
        custom,
    }
}

// ---------- Requests ----------

impl Client {
    pub fn new(site: &str, login: &Login) -> Client {
        use base64_lite::encode;
        Client {
            base: site.trim_end_matches('/').to_string(),
            agent: http_agent(),
            auth: format!("Basic {}", encode(format!("{}:{}", login.email, login.token))),
        }
    }

    fn get(&self, path: &str) -> Result<Value> {
        let t = std::time::Instant::now();
        let out = self.retrying(|| {
            let resp = self.agent.get(format!("{}{path}", self.base)).header("Authorization", &self.auth).header("Accept", "application/json").call()?;
            reply(resp)
        });
        perf("GET", path, t);
        out
    }

    fn send(&self, method: &str, path: &str, body: &Value) -> Result<Value> {
        let t = std::time::Instant::now();
        let url = format!("{}{path}", self.base);
        let out = self.retrying(|| {
            let req = match method {
                "POST" => self.agent.post(&url),
                "PUT" => self.agent.put(&url),
                _ => unreachable!("method {method}"),
            };
            let resp =
                req.header("Authorization", &self.auth).header("Accept", "application/json").header("Content-Type", "application/json").send_json(body)?;
            reply(resp)
        });
        perf(method, path, t);
        out
    }

    /// Runs `call`; on a 429 waits the Retry-After time once and tries again.
    /// A second 429 is returned as RateLimited.
    fn retrying(&self, call: impl Fn() -> Result<(u16, Duration, String)>) -> Result<Value> {
        let (code, wait, text) = call()?;
        if code != 429 {
            return self.finish(code, text);
        }
        std::thread::sleep(wait);
        let (code, _, text) = call()?;
        self.finish(code, text)
    }

    fn finish(&self, code: u16, text: String) -> Result<Value> {
        if !(200..300).contains(&code) {
            return Err(map_status(code, &text));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).map_err(|e| Error::Other(format!("bad JSON from Jira: {e}")))
    }

    pub fn myself(&self) -> Result<User> {
        let v = self.get("/rest/api/3/myself")?;
        user_at(&v).ok_or_else(|| Error::Other("myself: no displayName".into()))
    }

    /// All pages of a JQL search.
    pub fn search(&self, jql: &str, fields: &[&str]) -> Result<Vec<Issue>> {
        self.search_pages(jql, fields, usize::MAX)
    }

    /// Up to `max_pages` pages (tests use 1).
    pub fn search_pages(&self, jql: &str, fields: &[&str], max_pages: usize) -> Result<Vec<Issue>> {
        Ok(self.search_raw(jql, fields, None, max_pages)?.iter().map(parse_issue).collect())
    }

    /// The first `max` issues of a JQL search, in one request (the ⌘K palette's full-text search).
    pub fn search_first(&self, jql: &str, fields: &[&str], max: usize) -> Result<Vec<Issue>> {
        let v = self.send("POST", "/rest/api/3/search/jql", &json!({ "jql": jql, "fields": fields, "maxResults": max }))?;
        Ok(v["issues"].as_array().map(Vec::as_slice).unwrap_or_default().iter().map(parse_issue).collect())
    }

    /// Every page of a JQL search with each issue's changelog, as Jira's raw JSON (the Standup screen reads the
    /// status changes and comments out of it).
    pub fn search_with_changelog(&self, jql: &str, fields: &[&str]) -> Result<Vec<Value>> {
        self.search_raw(jql, fields, Some("changelog"), usize::MAX)
    }

    fn search_raw(&self, jql: &str, fields: &[&str], expand: Option<&str>, max_pages: usize) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        for _ in 0..max_pages {
            let mut body = json!({ "jql": jql, "fields": fields, "maxResults": 100 });
            if let Some(e) = expand {
                body["expand"] = json!(e);
            }
            if let Some(t) = &token {
                body["nextPageToken"] = json!(t);
            }
            let v = self.send("POST", "/rest/api/3/search/jql", &body)?;
            let page = v["issues"].as_array().map(Vec::as_slice).unwrap_or_default();
            out.extend(page.iter().cloned());
            let next = v["nextPageToken"].as_str().map(String::from);
            // Guard against a misbehaving server: stop on an empty page or a repeated token.
            if next.is_none() || page.is_empty() || next == token {
                break;
            }
            token = next;
        }
        Ok(out)
    }

    pub fn issue(&self, key: &str, extra_fields: &[&str]) -> Result<Issue> {
        let fields: Vec<&str> = LIST_FIELDS.iter().chain(DETAIL_FIELDS).chain(extra_fields).copied().collect();
        let v = self.get(&format!("/rest/api/3/issue/{key}?fields={}", fields.join(",")))?;
        Ok(parse_issue(&v))
    }

    pub fn transitions(&self, key: &str) -> Result<Vec<Transition>> {
        Ok(self.transitions_with_category(key)?.into_iter().map(|(t, _)| t).collect())
    }

    /// The issue's transitions, each with its target status's category key ("new", "indeterminate", "done").
    pub fn transitions_with_category(&self, key: &str) -> Result<Vec<(Transition, String)>> {
        let v = self.get(&format!("/rest/api/3/issue/{key}/transitions"))?;
        Ok(v["transitions"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|t| {
                        let tr = Transition {
                            id: str_at(t, &["id"]).unwrap_or_default(),
                            name: str_at(t, &["name"]).unwrap_or_default(),
                            to: str_at(t, &["to", "name"]).unwrap_or_default(),
                        };
                        (tr, str_at(t, &["to", "statusCategory", "key"]).unwrap_or_default())
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn resolutions(&self) -> Result<Vec<Resolution>> {
        let v = self.get("/rest/api/3/resolution")?;
        Ok(v.as_array()
            .map(|a| a.iter().map(|r| Resolution { id: str_at(r, &["id"]).unwrap_or_default(), name: str_at(r, &["name"]).unwrap_or_default() }).collect())
            .unwrap_or_default())
    }

    /// A user's avatar URL by account id, for users seen only in cached data.
    pub fn user_avatar_url(&self, account_id: &str) -> Result<Option<String>> {
        let v = self.get(&format!("/rest/api/3/user?accountId={}", urlencode(account_id)))?;
        user_at(&v);
        Ok(avatar_url_in(&v))
    }

    /// The raw bytes at `url` (an avatar image), at most 2 MB. Sends the Jira credentials only to the Jira site.
    pub fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let t = std::time::Instant::now();
        let mut req = self.agent.get(url).header("Accept", "image/png,image/jpeg,image/*");
        if same_origin(&self.base, url) {
            req = req.header("Authorization", &self.auth);
        }
        let mut resp = req.call()?;
        let code = resp.status().as_u16();
        let out = if (200..300).contains(&code) {
            resp.body_mut().with_config().limit(2 << 20).read_to_vec().map_err(Error::from)
        } else {
            Err(map_status(code, ""))
        };
        perf("GET", url, t);
        out
    }

    pub fn assignable_users(&self, project: &str) -> Result<Vec<User>> {
        let v = self.get(&format!("/rest/api/3/user/assignable/search?project={project}&maxResults=100"))?;
        Ok(v.as_array().map(|a| a.iter().filter_map(user_at).collect()).unwrap_or_default())
    }

    /// `states` is a comma list: "active,future".
    pub fn board_sprints(&self, board_id: u64, states: &str) -> Result<Vec<Sprint>> {
        let v = self.get(&format!("/rest/agile/1.0/board/{board_id}/sprint?state={states}&maxResults=50"))?;
        Ok(v["values"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|s| Sprint {
                        id: s["id"].as_u64().unwrap_or(0),
                        name: str_at(s, &["name"]).unwrap_or_default(),
                        state: str_at(s, &["state"]).unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Every issue type the user can create in `project`, all pages of Jira's create metadata.
    pub fn creatable_issue_types(&self, project: &str) -> Result<Vec<IssueType>> {
        let mut out = Vec::new();
        // A cap, so a server that ignores startAt cannot loop forever; 100 pages is 5000 types.
        for _ in 0..100 {
            let v = self.get(&format!("/rest/api/3/issue/createmeta/{}/issuetypes?startAt={}&maxResults=50", urlencode(project), out.len()))?;
            let page = v["issueTypes"].as_array().or_else(|| v["values"].as_array()).map(Vec::as_slice).unwrap_or_default();
            out.extend(page.iter().map(|t| IssueType {
                id: str_at(t, &["id"]).unwrap_or_default(),
                name: str_at(t, &["name"]).unwrap_or_default(),
                subtask: t["subtask"].as_bool().unwrap_or(false),
            }));
            let total = v["total"].as_u64().map(|n| n as usize);
            // Guard against a misbehaving server: an empty page always ends it.
            if page.is_empty() || v["isLast"].as_bool() == Some(true) || total.is_some_and(|n| out.len() >= n) {
                break;
            }
            if total.is_none() && v["isLast"].is_null() && page.len() < v["maxResults"].as_u64().unwrap_or(50) as usize {
                break;
            }
        }
        Ok(out)
    }

    pub fn transition(&self, key: &str, transition_id: &str, resolution_id: Option<&str>) -> Result<()> {
        check_writable()?;
        let mut body = json!({ "transition": { "id": transition_id } });
        if let Some(r) = resolution_id {
            body["fields"] = json!({ "resolution": { "id": r } });
        }
        self.send("POST", &format!("/rest/api/3/issue/{key}/transitions"), &body)?;
        Ok(())
    }

    pub fn rank(&self, key: &str, before: Option<&str>, after: Option<&str>) -> Result<()> {
        check_writable()?;
        let mut body = json!({ "issues": [key] });
        match (before, after) {
            (Some(b), _) => body["rankBeforeIssue"] = json!(b),
            (None, Some(a)) => body["rankAfterIssue"] = json!(a),
            (None, None) => return Err(Error::Other("rank: no neighbour given".into())),
        }
        self.send("PUT", "/rest/agile/1.0/issue/rank", &body)?;
        Ok(())
    }

    pub fn assign(&self, key: &str, account_id: Option<&str>) -> Result<()> {
        check_writable()?;
        self.send("PUT", &format!("/rest/api/3/issue/{key}/assignee"), &json!({ "accountId": account_id }))?;
        Ok(())
    }

    pub fn move_to_sprint(&self, sprint_id: u64, keys: &[&str]) -> Result<()> {
        check_writable()?;
        self.send("POST", &format!("/rest/agile/1.0/sprint/{sprint_id}/issue"), &json!({ "issues": keys }))?;
        Ok(())
    }

    pub fn move_to_backlog(&self, keys: &[&str]) -> Result<()> {
        check_writable()?;
        self.send("POST", "/rest/agile/1.0/backlog/issue", &json!({ "issues": keys }))?;
        Ok(())
    }

    pub fn update_fields(&self, key: &str, fields: Value) -> Result<()> {
        check_writable()?;
        self.send("PUT", &format!("/rest/api/3/issue/{key}"), &json!({ "fields": fields }))?;
        Ok(())
    }

    /// Adds one label server-side, leaving the issue's other labels alone even if they changed since our fetch.
    pub fn add_label(&self, key: &str, label: &str) -> Result<()> {
        check_writable()?;
        self.send("PUT", &format!("/rest/api/3/issue/{key}"), &json!({ "update": { "labels": [{ "add": label }] } }))?;
        Ok(())
    }

    /// Creates an issue and returns its key. `fields` is Jira's create `fields` object.
    pub fn create_issue(&self, fields: Value) -> Result<String> {
        check_writable()?;
        let v = self.send("POST", "/rest/api/3/issue", &json!({ "fields": fields }))?;
        str_at(&v, &["key"]).ok_or_else(|| Error::Other("create issue: Jira returned no key".into()))
    }

    pub fn add_comment(&self, key: &str, text: &str) -> Result<()> {
        check_writable()?;
        self.send("POST", &format!("/rest/api/3/issue/{key}/comment"), &json!({ "body": crate::adf::text_doc(text) }))?;
        Ok(())
    }
}

/// Status, Retry-After wait, and body text of a response.
fn reply(mut resp: ureq::http::Response<ureq::Body>) -> Result<(u16, Duration, String)> {
    let wait = retry_after(resp.headers().get("Retry-After").and_then(|v| v.to_str().ok()));
    Ok((resp.status().as_u16(), wait, resp.body_mut().read_to_string()?))
}

/// Retry-After in seconds: 2 when absent or not a number, at most 10.
fn retry_after(header: Option<&str>) -> Duration {
    Duration::from_secs(header.and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(2).min(10))
}

/// The error in plain words: Jira's own message without the "Jira returned 400: " in front, so a rejected field
/// reads "issuetype: Specify a valid issue type". Other codes keep theirs.
pub fn reason(e: &Error) -> String {
    let m = e.to_string();
    m.strip_prefix("Jira returned 400: ").map(String::from).unwrap_or(m)
}

pub fn needs_resolution(e: &Error) -> bool {
    e.to_string().to_ascii_lowercase().contains("resolution")
}

/// Jira rejected the JQL because a configured member (`assignee in (...)`) is not a user it knows.
pub fn is_unresolved_assignee(e: &Error) -> bool {
    matches!(e, Error::Other(m) if m.contains("does not exist for the field 'assignee'"))
}

fn perf(method: &str, path: &str, start: std::time::Instant) {
    if std::env::var_os("JIRA_TICKETS_PERF").is_some() {
        eprintln!("{method} {path} {:.0} ms", start.elapsed().as_secs_f64() * 1000.0);
    }
}

/// Minimal base64 so the crate has no extra dependency for one header.
mod base64_lite {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    pub fn encode(s: String) -> String {
        let b = s.as_bytes();
        let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
        for chunk in b.chunks(3) {
            let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &x)| acc | (x as u32) << (16 - 8 * i));
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::mpsc::{Receiver, channel};

    pub static WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Sets READ_ONLY for the guard's lifetime and always restores false on drop.
    pub(crate) struct ReadOnlyGuard;
    impl ReadOnlyGuard {
        pub(crate) fn new() -> Self {
            READ_ONLY.store(true, Ordering::Relaxed);
            ReadOnlyGuard
        }
    }
    impl Drop for ReadOnlyGuard {
        fn drop(&mut self) {
            READ_ONLY.store(false, Ordering::Relaxed);
        }
    }

    #[derive(Debug)]
    pub struct Recorded {
        pub method: String,
        pub path: String,
        pub body: String,
    }

    /// A tiny HTTP/1.1 server. Each route is (method, path prefix, JSON reply).
    /// Unmatched requests get 404. Answers one request per accepted connection.
    pub fn stub_server(routes: Vec<(&'static str, &'static str, serde_json::Value)>) -> (String, Receiver<Recorded>) {
        stub_server_throttled(routes.into_iter().map(|(m, p, v)| (m, p, v, 0)).collect())
    }

    /// Like `stub_server`, but each route first answers its last element's
    /// count of requests with 429 (Retry-After: 0).
    pub fn stub_server_throttled(routes: Vec<(&'static str, &'static str, serde_json::Value, usize)>) -> (String, Receiver<Recorded>) {
        serve(routes.into_iter().map(|(m, p, v, n)| (m, p, v, n, 200)).collect())
    }

    /// Like `stub_server`, but each route answers with its own HTTP status (400 for a rejected create).
    pub fn stub_server_codes(routes: Vec<(&'static str, &'static str, u16, serde_json::Value)>) -> (String, Receiver<Recorded>) {
        serve(routes.into_iter().map(|(m, p, code, v)| (m, p, v, 0, code)).collect())
    }

    fn serve(routes: Vec<(&'static str, &'static str, serde_json::Value, usize, u16)>) -> (String, Receiver<Recorded>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut throttle: Vec<usize> = routes.iter().map(|r| r.3).collect();
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                let (mut head_end, mut content_len) = (None, 0usize);
                loop {
                    let n = match s.read(&mut tmp) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    buf.extend_from_slice(&tmp[..n]);
                    if head_end.is_none()
                        && let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n")
                    {
                        head_end = Some(i + 4);
                        let head = String::from_utf8_lossy(&buf[..i]).to_string();
                        content_len = head
                            .lines()
                            .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0)))
                            .unwrap_or(0);
                    }
                    if let Some(h) = head_end
                        && buf.len() >= h + content_len
                    {
                        break;
                    }
                }
                let Some(h) = head_end else { continue };
                let head = String::from_utf8_lossy(&buf[..h]).to_string();
                let mut first = head.lines().next().unwrap_or("").split(' ');
                let method = first.next().unwrap_or("").to_string();
                let path = first.next().unwrap_or("").to_string();
                let body = String::from_utf8_lossy(&buf[h..h + content_len]).to_string();
                let route = routes.iter().position(|(m, p, _, _, _)| *m == method && path.starts_with(p));
                let _ = tx.send(Recorded { method, path, body });
                let (code, text) = match route {
                    Some(i) if throttle[i] > 0 => {
                        throttle[i] -= 1;
                        ("429 Too Many Requests\r\nRetry-After: 0".to_string(), "{}".to_string())
                    }
                    Some(i) if routes[i].4 == 200 => ("200 OK".to_string(), routes[i].2.to_string()),
                    Some(i) => (format!("{} Error", routes[i].4), routes[i].2.to_string()),
                    None => ("404 Not Found".to_string(), "{}".to_string()),
                };
                let _ = write!(s, "HTTP/1.1 {code}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
            }
        });
        (base, rx)
    }

    fn client(base: &str) -> Client {
        Client::new(base, &crate::auth::Login { email: "me@x.com".into(), token: "tok".into(), source: crate::auth::Source::Env })
    }

    fn fixture(name: &str) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap()).unwrap()
    }

    #[test]
    fn parses_search_page() {
        let page = fixture("search_page.json");
        let issues: Vec<Issue> = page["issues"].as_array().unwrap().iter().map(parse_issue).collect();
        assert_eq!(issues.len(), 2);
        let a = &issues[0];
        assert_eq!(a.key, "CI-9001");
        assert_eq!(a.status.name, "New");
        assert_eq!(a.status.category, "new");
        assert_eq!(a.parent_key.as_deref(), Some("CI-79"));
        assert_eq!(a.parent_id.as_deref(), Some("10002"));
        assert_eq!(a.parent_summary.as_deref(), Some("Cluster Operations (ongoing)"));
        assert_eq!(a.sprint.as_deref(), Some("Sprint 92"));
        assert_eq!(a.story_points, Some(3.0));
        assert_eq!(a.rank, "0|i0abcd:");
        assert!(a.assignee.is_none());
        let b = &issues[1];
        assert_eq!(b.assignee.as_ref().unwrap().display_name, "Ada Lovelace");
        assert_eq!(b.due_date.as_deref(), Some("2026-12-31"));
        assert!(b.sprint.is_none());
    }

    #[test]
    fn sprint_is_the_open_one_and_closed_only_is_backlog() {
        let page = fixture("sprint_history.json");
        let issues: Vec<Issue> = page["issues"].as_array().unwrap().iter().map(parse_issue).collect();
        assert_eq!(issues[0].key, "CI-9239");
        assert!(issues[0].sprint.is_none(), "only closed sprints: back in the backlog");
        assert_eq!(issues[1].sprint.as_deref(), Some("Sprint 91"), "a closed sprint and the active one");
        assert_eq!(issues[2].sprint.as_deref(), Some("Sprint 92"), "carried into a future sprint");
        let f = crate::filter::IssueFilter { sprint: crate::filter::SprintScope::Backlog, ..Default::default() };
        let keys: Vec<&str> = crate::filter::visible(&issues.iter().collect::<Vec<_>>(), &f, false, None).iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, ["CI-9239"]);
    }

    #[test]
    fn parses_detail_with_description_and_comments() {
        let v = fixture("issue_detail.json");
        let i = parse_issue(&v);
        assert!(i.description.is_some());
        assert_eq!(i.comments.len(), 1);
        assert_eq!(i.comments[0].author, "Ada Lovelace");
    }

    #[test]
    fn search_posts_jql_and_follows_pages() {
        // The stub answers the same page (with a nextPageToken) to every call,
        // so cap at 2 pages and check the second request carried the token.
        let page = serde_json::json!({ "issues": fixture("search_page.json")["issues"], "nextPageToken": "t2" });
        let (base, rx) = stub_server(vec![("POST", "/rest/api/3/search/jql", page)]);
        let c = client(&base);
        let got = c.search_pages("project = CI", &["summary"], 2).unwrap();
        assert_eq!(got.len(), 4);
        let first: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(first["jql"], "project = CI");
        assert_eq!(first["fields"][0], "summary");
        assert!(first.get("nextPageToken").is_none());
        let second: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(second["nextPageToken"], "t2");
    }

    #[test]
    fn search_first_asks_for_one_short_page() {
        let page = serde_json::json!({ "issues": fixture("search_page.json")["issues"], "nextPageToken": "t2" });
        let (base, rx) = stub_server(vec![("POST", "/rest/api/3/search/jql", page)]);
        let got = client(&base).search_first("text ~ \"disk*\"", &["summary"], 25).unwrap();
        assert_eq!(got.len(), 2);
        let body: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(body["maxResults"], 25);
        assert!(rx.try_recv().is_err(), "one request, the next page is not followed");
    }

    #[test]
    fn unauthorized_maps_to_error() {
        let (base, _rx) = stub_server(vec![]);
        let c = client(&base);
        // 404 from the stub is "Other"; a real 401 is tested through status mapping.
        assert!(matches!(c.myself(), Err(Error::Other(_))));
        assert!(matches!(map_status(401, "x"), Error::Unauthorized(_)));
        assert!(matches!(map_status(403, "x"), Error::Other(_)), "a 403 must not sign the user out");
        assert!(matches!(map_status(429, "x"), Error::RateLimited(_)));
    }

    #[test]
    fn retries_once_after_429() {
        let me = serde_json::json!({ "accountId": "a1", "displayName": "Me" });
        let (base, rx) = stub_server_throttled(vec![("GET", "/rest/api/3/myself", me.clone(), 1)]);
        assert_eq!(client(&base).myself().unwrap().display_name, "Me");
        assert_eq!(rx.try_iter().count(), 2, "one 429, one retry");
        let page = serde_json::json!({ "issues": [] });
        let (base, rx) = stub_server_throttled(vec![("POST", "/rest/api/3/search/jql", page, 1)]);
        assert!(client(&base).search("project = CI", &["summary"]).is_ok());
        assert_eq!(rx.try_iter().count(), 2);
        let (base, rx) = stub_server_throttled(vec![("GET", "/rest/api/3/myself", me, 2)]);
        assert!(matches!(client(&base).myself(), Err(Error::RateLimited(_))));
        assert_eq!(rx.try_iter().count(), 2, "retried only once");
    }

    #[test]
    fn retry_after_defaults_and_caps() {
        assert_eq!(retry_after(None), Duration::from_secs(2));
        assert_eq!(retry_after(Some("x")), Duration::from_secs(2));
        assert_eq!(retry_after(Some(" 3 ")), Duration::from_secs(3));
        assert_eq!(retry_after(Some("120")), Duration::from_secs(10));
    }

    #[test]
    fn transport_errors_map_to_network() {
        let e: Error = ureq::Error::Io(std::io::Error::other("connection refused")).into();
        assert!(matches!(&e, Error::Network(m) if m.starts_with("Network error: ")));
    }

    #[test]
    fn search_stops_on_empty_page_with_token() {
        let page = serde_json::json!({ "issues": [], "nextPageToken": "same" });
        let (base, _rx) = stub_server(vec![("POST", "/rest/api/3/search/jql", page)]);
        let got = client(&base).search("project = CI", &["summary"]).unwrap();
        assert!(got.is_empty());
    }

    #[test]
    fn avatar_urls_prefer_48_and_are_remembered() {
        let v = serde_json::json!({ "accountId": "av-1", "displayName": "A", "avatarUrls": { "16x16": "s16", "24x24": "s24", "48x48": "s48" } });
        assert_eq!(avatar_url_in(&v).as_deref(), Some("s48"));
        assert_eq!(avatar_url_in(&serde_json::json!({ "avatarUrls": { "16x16": "s16", "24x24": "s24" } })).as_deref(), Some("s24"));
        assert_eq!(avatar_url_in(&serde_json::json!({ "displayName": "A" })), None);
        assert_eq!(user_at(&v).unwrap().account_id, "av-1");
        assert_eq!(known_avatar_url("av-1").as_deref(), Some("s48"));
        assert_eq!(known_avatar_url("nobody"), None);
    }

    #[test]
    fn credentials_go_only_to_the_jira_site() {
        let base = "https://acme.atlassian.net";
        assert!(same_origin(base, "https://acme.atlassian.net/secure/useravatar?avatarId=1"));
        assert!(same_origin(base, "HTTPS://ACME.atlassian.net"));
        assert!(!same_origin(base, "https://avatar-management--avatars.us-west-2.prod.public.atl-paas.net/x.png"));
        assert!(!same_origin(base, "https://acme.atlassian.net.evil.com/x"));
        assert!(!same_origin(base, "http://acme.atlassian.net/x"));
        assert!(!same_origin(base, "not a url"));
        assert_eq!(urlencode("557058:f5-8_.~ x"), "557058%3Af5-8_.~%20x");
    }

    #[test]
    fn user_avatar_url_and_fetch_bytes() {
        let user = serde_json::json!({ "accountId": "557058:z", "displayName": "Z", "avatarUrls": { "48x48": "https://cdn.example/z.png" } });
        let (base, rx) = stub_server(vec![("GET", "/rest/api/3/user", user), ("GET", "/img/", serde_json::json!("PNGDATA"))]);
        let c = client(&base);
        assert_eq!(c.user_avatar_url("557058:z").unwrap().as_deref(), Some("https://cdn.example/z.png"));
        assert_eq!(rx.recv().unwrap().path, "/rest/api/3/user?accountId=557058%3Az");
        assert_eq!(c.fetch_bytes(&format!("{base}/img/a.png")).unwrap(), b"\"PNGDATA\"");
        assert!(c.fetch_bytes(&format!("{base}/missing")).is_err());
    }

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64_lite::encode("a".into()), "YQ==");
        assert_eq!(base64_lite::encode("ab".into()), "YWI=");
        assert_eq!(base64_lite::encode("abc".into()), "YWJj");
        assert_eq!(base64_lite::encode("me@x.com:tok".into()), "bWVAeC5jb206dG9r");
    }

    #[test]
    fn map_status_extracts_jira_errors() {
        let body = r#"{"errorMessages":["Issue does not exist"],"errors":{"resolution":"Resolution is required"}}"#;
        let msg = map_status(400, body).to_string();
        assert!(msg.contains("Issue does not exist"), "{msg}");
        assert!(msg.contains("resolution: Resolution is required"), "{msg}");
    }

    #[test]
    fn read_only_blocks_writes() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        {
            let _ro = ReadOnlyGuard::new();
            assert!(check_writable().is_err());
        }
        assert!(check_writable().is_ok());
    }

    #[test]
    fn transition_sends_id_and_optional_resolution() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = stub_server(vec![("POST", "/rest/api/3/issue/CI-1/transitions", serde_json::json!({}))]);
        let c = client(&base);
        c.transition("CI-1", "11", None).unwrap();
        let r = rx.recv().unwrap();
        let body: serde_json::Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(body["transition"]["id"], "11");
        assert!(body.get("fields").is_none());
        c.transition("CI-1", "31", Some("10000")).unwrap();
        let r = rx.recv().unwrap();
        let body: serde_json::Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(body["fields"]["resolution"]["id"], "10000");
    }

    #[test]
    fn rank_before_and_after() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = stub_server(vec![("PUT", "/rest/agile/1.0/issue/rank", serde_json::json!({}))]);
        let c = client(&base);
        c.rank("CI-1", Some("CI-2"), None).unwrap();
        let body: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(body["issues"][0], "CI-1");
        assert_eq!(body["rankBeforeIssue"], "CI-2");
        assert!(body.get("rankAfterIssue").is_none());
        c.rank("CI-1", None, Some("CI-3")).unwrap();
        let body: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(body["rankAfterIssue"], "CI-3");
        assert!(body.get("rankBeforeIssue").is_none());
        assert!(c.rank("CI-1", None, None).is_err());
        assert!(rx.try_recv().is_err(), "nothing was sent");
    }

    #[test]
    fn assign_and_unassign() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = stub_server(vec![("PUT", "/rest/api/3/issue/CI-1/assignee", serde_json::json!({}))]);
        let c = client(&base);
        c.assign("CI-1", Some("abc")).unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&rx.recv().unwrap().body).unwrap()["accountId"], "abc");
        c.assign("CI-1", None).unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&rx.recv().unwrap().body).unwrap()["accountId"].is_null());
    }

    #[test]
    fn sprint_moves_and_fields_and_comment() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = stub_server(vec![
            ("POST", "/rest/agile/1.0/sprint/55/issue", serde_json::json!({})),
            ("POST", "/rest/agile/1.0/backlog/issue", serde_json::json!({})),
            ("PUT", "/rest/api/3/issue/CI-1", serde_json::json!({})),
            ("POST", "/rest/api/3/issue/CI-1/comment", serde_json::json!({ "id": "1" })),
        ]);
        let c = client(&base);
        c.move_to_sprint(55, &["CI-1"]).unwrap();
        assert_eq!(rx.recv().unwrap().path, "/rest/agile/1.0/sprint/55/issue");
        c.move_to_backlog(&["CI-1"]).unwrap();
        assert_eq!(rx.recv().unwrap().path, "/rest/agile/1.0/backlog/issue");
        c.update_fields("CI-1", serde_json::json!({ "labels": ["PLAT"] })).unwrap();
        let body: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(body["fields"]["labels"][0], "PLAT");
        c.add_comment("CI-1", "hello\nworld").unwrap();
        let body: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(body["body"]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn add_label_is_an_atomic_update() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = stub_server(vec![("PUT", "/rest/api/3/issue/CI-1", serde_json::json!({}))]);
        let c = client(&base);
        {
            let _ro = ReadOnlyGuard::new();
            assert!(c.add_label("CI-1", "PLAT").is_err());
        }
        assert!(rx.try_recv().is_err(), "read-only sent nothing");
        c.add_label("CI-1", "PLAT").unwrap();
        let r = rx.recv().unwrap();
        assert_eq!((r.method.as_str(), r.path.as_str()), ("PUT", "/rest/api/3/issue/CI-1"));
        let body: serde_json::Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(body, serde_json::json!({ "update": { "labels": [{ "add": "PLAT" }] } }));
    }

    #[test]
    fn writes_blocked_in_read_only() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = stub_server(vec![]);
        let c = client(&base);
        {
            let _ro = ReadOnlyGuard::new();
            let err = c.transition("CI-1", "1", None).unwrap_err();
            assert!(err.to_string().contains("read-only"), "{err}");
        }
        assert!(rx.try_recv().is_err(), "nothing was sent");
    }

    #[test]
    fn create_issue_posts_fields_and_returns_the_key() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, rx) = stub_server(vec![("POST", "/rest/api/3/issue", serde_json::json!({ "id": "10001", "key": "CI-42" }))]);
        let c = client(&base);
        {
            let _ro = ReadOnlyGuard::new();
            assert!(is_read_only(&c.create_issue(serde_json::json!({})).unwrap_err()));
        }
        assert!(rx.try_recv().is_err(), "read-only sent nothing");
        let fields = serde_json::json!({ "project": { "key": "CI" }, "summary": "Do it", "issuetype": { "name": "Task" }, "labels": ["PLAT"] });
        assert_eq!(c.create_issue(fields.clone()).unwrap(), "CI-42");
        let r = rx.recv().unwrap();
        assert_eq!((r.method.as_str(), r.path.as_str()), ("POST", "/rest/api/3/issue"));
        assert_eq!(serde_json::from_str::<serde_json::Value>(&r.body).unwrap(), serde_json::json!({ "fields": fields }));
    }

    #[test]
    fn create_issue_without_a_key_is_an_error() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (base, _rx) = stub_server(vec![("POST", "/rest/api/3/issue", serde_json::json!({ "id": "10001" }))]);
        assert!(client(&base).create_issue(serde_json::json!({})).unwrap_err().to_string().contains("no key"));
    }

    #[test]
    fn search_with_changelog_asks_for_the_changelog_and_keeps_raw_json() {
        let page = serde_json::json!({ "issues": [{ "key": "CI-1", "changelog": { "histories": [] }, "fields": { "summary": "a" } }] });
        let (base, rx) = stub_server(vec![("POST", "/rest/api/3/search/jql", page)]);
        let got = client(&base).search_with_changelog("updated >= \"2026-10-30\"", &["summary"]).unwrap();
        assert_eq!(got.len(), 1);
        assert!(got[0]["changelog"]["histories"].is_array());
        let body: serde_json::Value = serde_json::from_str(&rx.recv().unwrap().body).unwrap();
        assert_eq!(body["expand"], "changelog");
        assert_eq!(body["fields"][0], "summary");
    }

    #[test]
    fn detects_unresolved_assignee() {
        let e = Error::Other("Jira returned 400: The value 'Gone Person' does not exist for the field 'assignee'.".into());
        assert!(is_unresolved_assignee(&e));
        assert!(!is_unresolved_assignee(&Error::Other("Jira returned 400: The value 'x' does not exist for the field 'labels'.".into())));
        assert!(!is_unresolved_assignee(&Error::Network("does not exist for the field 'assignee'".into())), "only Jira's own reply");
        assert!(!is_unresolved_assignee(&Error::Other("boom".into())));
    }

    #[test]
    fn creatable_issue_types_follow_every_page() {
        let t = |id: &str, name: &str, sub: bool| serde_json::json!({ "id": id, "name": name, "subtask": sub, "description": "x" });
        let p1 = serde_json::json!({ "issueTypes": [t("1", "Task", false), t("2", "Bug", false)], "startAt": 0, "maxResults": 2, "total": 3 });
        let p2 = serde_json::json!({ "issueTypes": [t("5", "Sub-task", true)], "startAt": 2, "maxResults": 2, "total": 3 });
        let (base, rx) = stub_server(vec![
            ("GET", "/rest/api/3/issue/createmeta/CI/issuetypes?startAt=0&", p1),
            ("GET", "/rest/api/3/issue/createmeta/CI/issuetypes?startAt=2&", p2),
        ]);
        let got = client(&base).creatable_issue_types("CI").unwrap();
        let names: Vec<(&str, &str, bool)> = got.iter().map(|t| (t.id.as_str(), t.name.as_str(), t.subtask)).collect();
        assert_eq!(names, [("1", "Task", false), ("2", "Bug", false), ("5", "Sub-task", true)]);
        let paths: Vec<String> = rx.try_iter().map(|r| r.path).collect();
        assert_eq!(
            paths,
            ["/rest/api/3/issue/createmeta/CI/issuetypes?startAt=0&maxResults=50", "/rest/api/3/issue/createmeta/CI/issuetypes?startAt=2&maxResults=50"]
        );
    }

    #[test]
    fn creatable_issue_types_stop_on_is_last_a_short_page_or_an_empty_one() {
        let one = serde_json::json!([{ "id": "1", "name": "Task" }]);
        for page in [
            serde_json::json!({ "values": one, "isLast": true, "total": 99 }),
            serde_json::json!({ "issueTypes": one, "maxResults": 50 }),
            serde_json::json!({ "issueTypes": [], "total": 99 }),
        ] {
            let (base, rx) = stub_server(vec![("GET", "/rest/api/3/issue/createmeta/", page.clone())]);
            let got = client(&base).creatable_issue_types("CI").unwrap();
            assert!(got.len() <= 1, "{page}");
            assert_eq!(rx.try_iter().count(), 1, "one request: {page}");
        }
        let (base, _rx) = stub_server(vec![]);
        assert!(client(&base).creatable_issue_types("CI").is_err(), "a 404 is an error, not no types");
    }

    #[test]
    fn creatable_issue_types_is_a_read_in_read_only_mode() {
        let _g = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ro = ReadOnlyGuard::new();
        let (base, _rx) =
            stub_server(vec![("GET", "/rest/api/3/issue/createmeta/", serde_json::json!({ "issueTypes": [{ "id": "1", "name": "Task" }], "isLast": true }))]);
        assert_eq!(client(&base).creatable_issue_types("CI").unwrap().len(), 1);
    }

    #[test]
    fn reasons_drop_the_400_prefix_only() {
        let e = map_status(400, r#"{"errorMessages":[],"errors":{"issuetype":"Specify a valid issue type"}}"#);
        assert_eq!(reason(&e), "issuetype: Specify a valid issue type");
        let e = map_status(400, r#"{"errors":{"issuetype":"Specify a valid issue type"}}"#);
        assert_eq!(reason(&e), "issuetype: Specify a valid issue type", "no errorMessages array at all");
        assert_eq!(reason(&Error::Other("Jira returned 403: no".into())), "Jira returned 403: no");
        assert_eq!(reason(&Error::Network("Network error: refused".into())), "Network error: refused");
    }

    #[test]
    fn detects_resolution_requirement() {
        assert!(needs_resolution(&Error::Other("Jira returned 400: resolution: Resolution is required".into())));
        assert!(!needs_resolution(&Error::Other("boom".into())));
    }
}
