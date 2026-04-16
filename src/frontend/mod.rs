use crate::{
    RepoList,
    commit::CommitInfo,
    package::{PackageBuildStatus, PackageEnum},
    repo::RepoInfo,
};
use gloo_timers::callback::Interval;
use gloo_timers::future::TimeoutFuture;
use gluesql::gluesql_memory_storage::MemoryStorage;
use gluesql::prelude::*;
use serde_json;
use std::sync::Arc;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    Event, HtmlElement, HtmlTextAreaElement, KeyboardEvent, Response, console, js_sys::Date,
};
use yew::prelude::*;

const COPY_ICON: &str = "\u{f0c5}";
const DEFAULT_SQL_QUERY: &str = r#"SELECT repo, repo_status, package_path, package_status, result, commit_message
FROM package_list as p
WHERE package_status not like 'UnsupportedArchitecture' and commit_timestamp_millis = (
    SELECT MAX(commit_timestamp_millis)
    FROM package_list as inner
    WHERE p.repo = inner.repo and p.package_path = inner.package_path
)
ORDER BY repo, package_path"#;

// Fetch the repo list via Fetch API and return deserialized RepoList
async fn fetch_repos() -> Result<RepoList, String> {
    let window = web_sys::window().ok_or_else(|| "no window available".to_string())?;
    let location = window.location();
    let protocol = location.protocol().map_err(|_| "no protocol".to_string())?;
    let host = location.host().map_err(|_| "no host".to_string())?;
    let url = format!("{}//{}/repos", protocol, host);
    let resp_value = JsFuture::from(window.fetch_with_str(&url))
        .await
        .map_err(|e| format!("fetch failed: {e:?}"))?;
    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| "failed to cast response".to_string())?;

    let text_promise = resp
        .text()
        .map_err(|e| format!("response.text() failed: {e:?}"))?;
    let text_js = JsFuture::from(text_promise)
        .await
        .map_err(|e| format!("awaiting text failed: {e:?}"))?;

    let text = text_js
        .as_string()
        .ok_or_else(|| "response not text".to_string())?;

    serde_json::from_str(&text).map_err(|e| format!("failed to parse json: {e}"))
}

#[derive(Debug)]
pub struct Package<'a> {
    repo: &'a RepoInfo,
    commit: &'a CommitInfo,
    pkg: &'a PackageEnum,
}

#[derive(Properties, PartialEq)]
struct TableRowProps {
    repo_url: String,
    package_path: String,
    branch: String,
    commit_message: String,
    status_class: String,
    repo_debug: String,
    commit_debug: String,
    pkg_debug: String,
}

#[function_component]
fn TableRow(props: &TableRowProps) -> Html {
    let expanded = use_state(|| false);
    let toggle = {
        let expanded = expanded.clone();
        Callback::from(move |_| {
            expanded.set(!*expanded);
        })
    };

    html! {
        <>
            <tr onclick={toggle} class="table-row-hover" style="cursor: pointer; border-bottom: 1px solid rgba(255, 255, 255, 0.08);">
                <td style="padding: 12px; color: var(--text);">{ &props.repo_url }</td>
                <td style="padding: 12px; font-family: monospace; font-size: 0.9em; color: var(--text);">{ &props.package_path }</td>
                <td style="padding: 12px; color: var(--text);">{ &props.branch }</td>
                <td style="padding: 12px; color: var(--muted);">{ &props.commit_message }</td>
                <td style="padding: 12px; text-align: center;">
                    <span style={format!("display: inline-block; width: 12px; height: 12px; border-radius: 50%; {}",
                        match props.status_class.as_str() {
                            "status-success" => "background-color: #4caf50;",
                            "status-failed" => "background-color: #f44336;",
                            "status-building" => "background-color: #ff9800;",
                            "status-pending" => "background-color: #2196f3;",
                            _ => "background-color: #9e9e9e;",
                        }
                    )} title={props.status_class.clone()}></span>
                </td>
            </tr>
            if *expanded {
                <tr>
                    <td colspan="5" style="background: var(--card-strong); padding: 10px; border-bottom: 1px solid rgba(255, 255, 255, 0.08);">
                        <details open={true}>
                            <summary><strong style="color: var(--text);">{ "Repository Debug Info" }</strong></summary>
                            <pre style="overflow-x: auto; white-space: pre-wrap; color: var(--muted); background: var(--card); padding: 8px; border-radius: 4px; margin-top: 8px;">{ &props.repo_debug }</pre>
                        </details>
                        <details open={true}>
                            <summary><strong style="color: var(--text);">{ "Commit Debug Info" }</strong></summary>
                            <pre style="overflow-x: auto; white-space: pre-wrap; color: var(--muted); background: var(--card); padding: 8px; border-radius: 4px; margin-top: 8px;">{ &props.commit_debug }</pre>
                        </details>
                        <details open={true}>
                            <summary><strong style="color: var(--text);">{ "Package Debug Info" }</strong></summary>
                            <pre style="overflow-x: auto; white-space: pre-wrap; color: var(--muted); background: var(--card); padding: 8px; border-radius: 4px; margin-top: 8px;">{ &props.pkg_debug }</pre>
                        </details>
                    </td>
                </tr>
            }
        </>
    }
}

#[derive(Clone, PartialEq)]
pub struct SqlResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

fn escape_sql(value: &str) -> String {
    value.replace('\'', "''")
}

fn highlight_sql(sql: &str) -> Vec<Html> {
    const KEYWORDS: [&str; 37] = [
        "SELECT", "FROM", "WHERE", "AND", "OR", "LIMIT", "ORDER", "BY", "GROUP", "HAVING", "AS",
        "JOIN", "LEFT", "RIGHT", "INNER", "OUTER", "ON", "DISTINCT", "COUNT", "SUM", "AVG", "MIN",
        "MAX", "INSERT", "INTO", "VALUES", "UPDATE", "SET", "DELETE", "CREATE", "TABLE", "DROP",
        "ALTER", "ADD", "NULL", "IS", "NOT",
    ];

    let mut out: Vec<Html> = Vec::new();
    let mut chars = sql.chars().peekable();

    while let Some(ch) = chars.peek().copied() {
        if ch.is_ascii_whitespace() {
            let mut buf = String::new();
            while let Some(c) = chars.peek().copied() {
                if c.is_ascii_whitespace() {
                    buf.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(Html::from(buf));
            continue;
        }

        if ch == '-' {
            let mut look = chars.clone();
            look.next();
            if let Some('-') = look.next() {
                let mut buf = String::new();
                buf.push('-');
                buf.push('-');
                chars.next();
                chars.next();
                while let Some(c) = chars.peek().copied() {
                    buf.push(c);
                    chars.next();
                    if c == '\n' {
                        break;
                    }
                }
                out.push(html! { <span class="sql-comment">{ buf }</span> });
                continue;
            }
        }

        if ch == '/' {
            let mut look = chars.clone();
            look.next();
            if let Some('*') = look.next() {
                let mut buf = String::new();
                buf.push('/');
                buf.push('*');
                chars.next();
                chars.next();
                while let Some(c) = chars.next() {
                    buf.push(c);
                    if c == '*' {
                        if let Some('/') = chars.peek().copied() {
                            buf.push('/');
                            chars.next();
                            break;
                        }
                    }
                }
                out.push(html! { <span class="sql-comment">{ buf }</span> });
                continue;
            }
        }

        if ch == '\'' {
            let mut buf = String::new();
            buf.push('\'');
            chars.next();
            while let Some(c) = chars.next() {
                buf.push(c);
                if c == '\'' {
                    if let Some('\'') = chars.peek().copied() {
                        buf.push('\'');
                        chars.next();
                        continue;
                    }
                    break;
                }
            }
            out.push(html! { <span class="sql-str">{ buf }</span> });
            continue;
        }

        if ch.is_ascii_digit() {
            let mut buf = String::new();
            while let Some(c) = chars.peek().copied() {
                if c.is_ascii_digit() || c == '.' {
                    buf.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(html! { <span class="sql-num">{ buf }</span> });
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut buf = String::new();
            while let Some(c) = chars.peek().copied() {
                if c.is_ascii_alphanumeric() || c == '_' {
                    buf.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            let upper = buf.to_ascii_uppercase();
            if KEYWORDS.contains(&upper.as_str()) {
                out.push(html! { <span class="sql-kw">{ buf }</span> });
            } else {
                out.push(html! { <span class="sql-ident">{ buf }</span> });
            }
            continue;
        }

        let mut buf = String::new();
        buf.push(ch);
        chars.next();
        out.push(html! { <span class="sql-op">{ buf }</span> });
    }

    out
}

fn render_sql_value(
    column: &str,
    value: &str,
    copy_handler: &Callback<(String, String)>,
    copy_token: &Option<String>,
) -> Html {
    let copied_value = value.to_string();
    match column.to_lowercase().as_str() {
        "repo" => {
            let token = format!("repo:{}", copied_value);
            let token_for_button = token.clone();
            let url = if value.starts_with("http://") || value.starts_with("https://") {
                value.to_string()
            } else {
                format!("https://{}", value)
            };
            html! {
                <td style="padding: 10px 12px; color: var(--text); white-space: nowrap; min-width: 0;">
                    /* This inner div handles the layout without breaking the table logic */
                    <div style="display: flex; gap: 8px; align-items: center; width: 100%;">
                        <span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 1 1 auto;">
                            <a href={url.clone()} target="_blank" rel="noreferrer" style="color: var(--accent);">{ value }</a>
                        </span>
                        <span style="position: relative; display: inline-flex; align-items: center; flex-shrink: 0;">
                            <button
                                type="button"
                                onclick={copy_handler.reform(move |_| (copied_value.clone(), token_for_button.clone()))}
                                class="copy-button"
                                style="background: transparent; border: none; color: var(--accent); cursor: pointer;"
                            >
                                { COPY_ICON }
                            </button>
                            { if copy_token.as_ref() == Some(&token) {
                                html! {
                                    <span style="position: absolute; top: -28px; right: 0; background: rgba(15, 23, 52, 0.95); color: var(--accent); padding: 4px 8px; border-radius: 6px; font-size: 0.8rem; white-space: nowrap; z-index: 10;">
                                        { "Copied" }
                                    </span>
                                }
                            } else {
                                html! {}
                            }}
                        </span>
                    </div>
                </td>
            }
        }
        "package_status" => {
            let status_class = match value.to_lowercase().as_str() {
                s if s.contains("success") => "status-success",
                s if s.contains("failed")
                    || s.contains("failure")
                    || s.contains("unsupportedarchitecture") =>
                {
                    "status-failed"
                }
                s if s.contains("building") || s.contains("running") => "status-building",
                s if s.contains("pending")
                    || s.contains("queued")
                    || s.contains("waitingforbuild") =>
                {
                    "status-pending"
                }
                _ => "status-unknown",
            };
            html! {
                <td style="padding: 10px 12px;">
                    <span class={classes!("status-indicator", status_class)}>{ value }</span>
                </td>
            }
        }
        "repo_status" | "commit_status" => {
            let status_class = match value.to_lowercase().as_str() {
                s if s.contains("success") => "status-success",
                s if s.contains("failed")
                    || s.contains("failure")
                    || s.contains("unsupportedarchitecture") =>
                {
                    "status-failed"
                }
                s if s.contains("building")
                    || s.contains("running")
                    || s.contains("pulling")
                    || s.contains("cloning")
                    || s.contains("opening")
                    || s.contains("polling")
                    || s.contains("gettingpackages") =>
                {
                    "status-building"
                }
                s if s.contains("idle")
                    || s.contains("pending")
                    || s.contains("queued")
                    || s.contains("waitingforbuild") =>
                {
                    "status-pending"
                }
                _ => "status-unknown",
            };
            html! {
                <td style="padding: 10px 12px;">
                    <span class={classes!("status-indicator", status_class)}>{ value }</span>
                </td>
            }
        }
        "repo_flake_url" | "commit_flake_url" | "package_flake_url" => {
            let token = format!("{}:{}", column, copied_value);
            let token_for_button = token.clone();
            html! {
                <td style="padding: 10px 12px;">
                    <div style="display: flex; gap: 8px; align-items: center; white-space: nowrap; min-width: 0;">
                        <span style="color: var(--accent); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 1 1 auto;">{ value }</span>
                        <span style="position: relative; display: inline-flex; align-items: center;">
                            <button type="button" onclick={copy_handler.reform(move |_| (copied_value.clone(), token_for_button.clone()))} class="copy-button" style="background: transparent; border: none; color: var(--accent); cursor: pointer;">{ COPY_ICON }</button>
                            { if copy_token.as_ref() == Some(&token) {
                                html! {
                                    <span style="position: absolute; top: -28px; right: 0; background: rgba(15, 23, 52, 0.95); color: var(--accent); padding: 4px 8px; border-radius: 6px; font-size: 0.8rem; white-space: nowrap; z-index: 10;">{ "Copied" }</span>
                                }
                            } else {
                                html! {}
                            }}
                        </span>
                    </div>
                </td>
            }
        }
        "result" => {
            if value.is_empty() {
                html! { <td style="padding: 10px 12px; color: var(--muted); white-space: nowrap;">{ "-" }</td> }
            } else {
                let linkable = value.starts_with("http://")
                    || value.starts_with("https://")
                    || value.starts_with("file://")
                    || value.starts_with('/')
                    || value.starts_with("./")
                    || value.starts_with("../");

                let is_error_value = value.to_lowercase().starts_with("failed:")
                    || value
                        .to_lowercase()
                        .starts_with("unsupported architecture:");
                let text_color = if is_error_value {
                    "#f87171"
                } else {
                    "var(--text)"
                };

                if linkable {
                    let href = value.to_string();
                    let copied_value = value.to_string();
                    let token = format!("result:{}", copied_value);
                    let token_for_button = token.clone();
                    html! {
                        <td style="padding: 10px 12px;">
                            <div style="display: flex; gap: 8px; align-items: center; white-space: nowrap; min-width: 0;">
                                <a href={href.clone()} target="_blank" rel="noreferrer" style="color: var(--accent); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 1 1 auto;">{ value }</a>
                                <span style="position: relative; display: inline-flex; align-items: center;">
                                    <button type="button" onclick={copy_handler.reform(move |_| (copied_value.clone(), token_for_button.clone()))} class="copy-button" style="background: transparent; border: none; color: var(--accent); cursor: pointer;">{ COPY_ICON }</button>
                                    { if copy_token.as_ref() == Some(&token) {
                                        html! {
                                            <span style="position: absolute; top: -28px; right: 0; background: rgba(15, 23, 52, 0.95); color: var(--accent); padding: 4px 8px; border-radius: 6px; font-size: 0.8rem; white-space: nowrap; z-index: 10;">{ "Copied" }</span>
                                        }
                                    } else {
                                        html! {}
                                    }}
                                </span>
                            </div>
                        </td>
                    }
                } else {
                    let token = format!("result:{}", copied_value);
                    let token_for_button = token.clone();
                    html! {
                        <td style={format!("padding: 10px 12px; color: {};", text_color)}>
                            <div style="display: flex; gap: 8px; align-items: center; white-space: nowrap; min-width: 0;">
                                <span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; flex: 1 1 auto;">{ value }</span>
                                <span style="position: relative; display: inline-flex; align-items: center;">
                                    <button type="button" onclick={copy_handler.reform(move |_| (copied_value.clone(), token_for_button.clone()))} class="copy-button" style="background: transparent; border: none; color: var(--accent); cursor: pointer;">{ COPY_ICON }</button>
                                    { if copy_token.as_ref() == Some(&token) {
                                        html! {
                                            <span style="position: absolute; top: -28px; right: 0; background: rgba(15, 23, 52, 0.95); color: var(--accent); padding: 4px 8px; border-radius: 6px; font-size: 0.8rem; white-space: nowrap; z-index: 10;">{ "Copied" }</span>
                                        }
                                    } else {
                                        html! {}
                                    }}
                                </span>
                            </div>
                        </td>
                    }
                }
            }
        }
        _ => {
            html! { <td style="padding: 10px 12px; color: var(--text); white-space: nowrap;">{ value }</td> }
        }
    }
}

fn build_package_rows(
    repos: &RepoList,
) -> Vec<(
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
)> {
    let mut package_list: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = vec![];

    for repo in repos.0.0.iter() {
        for (_hash, commit) in repo.commits.0.iter() {
            for pkg in commit.packages.0.iter() {
                let repo_url = repo.repo.url.clone();
                let repo_flake_url = repo.flake_url.clone();
                let repo_status = format!("{:?}", &repo.status.0);
                let commit_hash = commit.hash.clone();
                let commit_flake_url = commit.flake_url.clone();
                let commit_status = format!("{:?}", &commit.status.0);
                let commit_timestamp = format_commit_timestamp(commit.unix_secs);
                let commit_timestamp_millis = commit.unix_secs.saturating_mul(1000).to_string();

                let (
                    package_path,
                    package_type,
                    package_description,
                    package_flake_url,
                    arch,
                    result,
                    status_text,
                ) = match pkg {
                    PackageEnum::Derivation(arc_wrapper) => {
                        let status = arc_wrapper.0.status.0.clone();
                        let result = match &status {
                            PackageBuildStatus::Success(path) => path.clone(),
                            PackageBuildStatus::Failed(error) => format!("failed: {}", error),
                            PackageBuildStatus::UnsupportedArchitecture(arch) => {
                                format!("unsupported architecture: {}", arch)
                            }
                            _ => String::new(),
                        };
                        let status_text = status_label(&status);
                        (
                            arc_wrapper.0.path.clone(),
                            arc_wrapper.0.pkg_type.clone(),
                            arc_wrapper.0.description.clone(),
                            arc_wrapper.0.flake_url.clone(),
                            arc_wrapper.0.arch.clone(),
                            result,
                            status_text,
                        )
                    }
                    PackageEnum::NixosConfig(arc_wrapper) => {
                        let status = arc_wrapper.0.status.0.clone();
                        let result = match &status {
                            PackageBuildStatus::Success(path) => path.clone(),
                            PackageBuildStatus::Failed(error) => format!("failed: {}", error),
                            PackageBuildStatus::UnsupportedArchitecture(arch) => {
                                format!("unsupported architecture: {}", arch)
                            }
                            _ => String::new(),
                        };
                        let status_text = status_label(&status);
                        (
                            arc_wrapper.0.path.clone(),
                            arc_wrapper.0.pkg_type.clone(),
                            String::new(),
                            arc_wrapper.0.flake_url.clone(),
                            "N/A".to_string(),
                            result,
                            status_text,
                        )
                    }
                };

                let branch = repo
                    .branch_commit_hashes
                    .iter()
                    .find_map(|(branch, hashes)| {
                        if hashes.0.contains(&commit.hash) {
                            Some(branch.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "-".to_string());

                package_list.push((
                    repo_url,
                    repo_flake_url,
                    repo_status,
                    package_path,
                    package_type,
                    package_description,
                    package_flake_url,
                    branch,
                    commit_hash,
                    commit.message.clone(),
                    commit_flake_url,
                    commit_status,
                    commit_timestamp,
                    commit_timestamp_millis,
                    arch,
                    result,
                    status_text,
                ));
            }
        }
    }

    package_list
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Bool(v) => v.to_string(),
        Value::I8(v) => v.to_string(),
        Value::I16(v) => v.to_string(),
        Value::I32(v) => v.to_string(),
        Value::I64(v) => v.to_string(),
        Value::I128(v) => v.to_string(),
        Value::U8(v) => v.to_string(),
        Value::U16(v) => v.to_string(),
        Value::U32(v) => v.to_string(),
        Value::U64(v) => v.to_string(),
        Value::U128(v) => v.to_string(),
        Value::F32(v) => v.to_string(),
        Value::F64(v) => v.to_string(),
        Value::Decimal(v) => v.to_string(),
        Value::Str(v) => v.clone(),
        Value::Bytea(v) => format!("BYTEA({:?})", v),
        Value::Inet(v) => v.to_string(),
        Value::Date(v) => v.to_string(),
        Value::Timestamp(v) => v.to_string(),
        Value::Time(v) => v.to_string(),
        Value::Interval(v) => format!("{:?}", v),
        Value::Uuid(v) => v.to_string(),
        Value::Map(v) => format!("{:?}", v),
        Value::List(v) => format!("{:?}", v),
        Value::Point(v) => format!("{:?}", v),
        Value::Null => "NULL".to_string(),
    }
}

fn status_label(status: &PackageBuildStatus) -> String {
    match status {
        PackageBuildStatus::Idle => "Idle".to_string(),
        PackageBuildStatus::UnsupportedArchitecture(_) => "UnsupportedArchitecture".to_string(),
        PackageBuildStatus::WaitingForBuild => "WaitingForBuild".to_string(),
        PackageBuildStatus::Building => "Building".to_string(),
        PackageBuildStatus::Success(_) => "Success".to_string(),
        PackageBuildStatus::Failed(_) => "Failed".to_string(),
    }
}

fn format_commit_timestamp(unix_secs: i64) -> String {
    let millis = (unix_secs as f64) * 1000.0;
    let date = Date::new(&JsValue::from_f64(millis));
    date.to_locale_string("en-US", &JsValue::undefined()).into()
}

async fn execute_sql_query(repos: &RepoList, sql: &str) -> Result<SqlResult, String> {
    let mut glue = Glue::new(MemoryStorage::default());

    glue.execute(
        "CREATE TABLE package_list (repo TEXT, repo_flake_url TEXT, repo_status TEXT, package_path TEXT, package_type TEXT, package_description TEXT, package_flake_url TEXT, branch TEXT, commit_hash TEXT, commit_message TEXT, commit_flake_url TEXT, commit_status TEXT, commit_timestamp TEXT, commit_timestamp_millis TEXT, arch TEXT, result TEXT, package_status TEXT);",
    )
    .await
    .map_err(|e| format!("SQL engine error: {e}"))?;

    for (
        repo,
        repo_flake_url,
        repo_status,
        package_path,
        package_type,
        package_description,
        package_flake_url,
        branch,
        commit_hash,
        commit_message,
        commit_flake_url,
        commit_status,
        commit_timestamp,
        commit_timestamp_millis,
        arch,
        result,
        status,
    ) in build_package_rows(repos)
    {
        let insert = format!(
            "INSERT INTO package_list VALUES ('{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}','{}');",
            escape_sql(&repo),
            escape_sql(&repo_flake_url),
            escape_sql(&repo_status),
            escape_sql(&package_path),
            escape_sql(&package_type),
            escape_sql(&package_description),
            escape_sql(&package_flake_url),
            escape_sql(&branch),
            escape_sql(&commit_hash),
            escape_sql(&commit_message),
            escape_sql(&commit_flake_url),
            escape_sql(&commit_status),
            escape_sql(&commit_timestamp),
            escape_sql(&commit_timestamp_millis),
            escape_sql(&arch),
            escape_sql(&result),
            escape_sql(&status),
        );
        glue.execute(&insert)
            .await
            .map_err(|e| format!("SQL engine error: {e}"))?;
    }

    let payloads = glue
        .execute(sql)
        .await
        .map_err(|e| format!("SQL error: {e}"))?;
    let payload = payloads
        .into_iter()
        .last()
        .ok_or_else(|| "No SQL payload returned".to_string())?;

    match payload {
        Payload::Select { labels, rows } => {
            let columns: Vec<String> = labels.into_iter().collect();
            let rows = rows
                .into_iter()
                .map(|row| row.into_iter().map(|val| value_to_string(&val)).collect())
                .collect();
            Ok(SqlResult { columns, rows })
        }
        payload => Err(format!("Unsupported result payload: {payload:?}")),
    }
}

#[function_component]
fn App() -> Html {
    let repolist = use_state(|| None::<Result<Arc<RepoList>, String>>);
    let sql_query = use_state(|| DEFAULT_SQL_QUERY.to_string());
    let active_query_ref = use_mut_ref(|| DEFAULT_SQL_QUERY.to_string());
    let sql_result = use_state(|| None::<Result<SqlResult, String>>);
    let copy_token = use_state(|| None::<String>);
    let fetch_error = use_state(|| None::<String>);
    let sql_pre_ref = use_node_ref();

    let on_sql_scroll = {
        let sql_pre_ref = sql_pre_ref.clone();
        Callback::from(move |event: Event| {
            if let Some(textarea) = event
                .target()
                .and_then(|target| target.dyn_into::<HtmlTextAreaElement>().ok())
            {
                if let Some(pre) = sql_pre_ref.cast::<HtmlElement>() {
                    pre.set_scroll_top(textarea.scroll_top());
                    pre.set_scroll_left(textarea.scroll_left());
                }
            }
        })
    };

    let copy_handler = {
        let copy_token = copy_token.clone();
        Callback::from(move |(value, token): (String, String)| {
            let copy_token = copy_token.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Some(window) = web_sys::window() {
                    let clipboard = window.navigator().clipboard();
                    let promise = clipboard.write_text(&value);
                    let _ = JsFuture::from(promise).await;
                }
                copy_token.set(Some(token));
                TimeoutFuture::new(1500).await;
                copy_token.set(None);
            });
        })
    };

    let on_sql_input = {
        let sql_query = sql_query.clone();
        Callback::from(move |event: InputEvent| {
            if let Some(textarea) = event
                .target()
                .and_then(|target| target.dyn_into::<HtmlTextAreaElement>().ok())
            {
                sql_query.set(textarea.value());
            }
        })
    };

    let run_sql = {
        let data = repolist.clone();
        let sql_query = sql_query.clone();
        let active_query_ref = active_query_ref.clone();
        let sql_result = sql_result.clone();
        let fetch_error = fetch_error.clone();
        Callback::from(move |_: ()| {
            let data = data.clone();
            let query = (*sql_query).clone();
            let active_query_ref = active_query_ref.clone();
            let sql_result = sql_result.clone();
            let fetch_error = fetch_error.clone();
            wasm_bindgen_futures::spawn_local(async move {
                console::log_1(&format!("run_sql: {}", query).into());
                *active_query_ref.borrow_mut() = query.clone();
                let result = match &*data {
                    Some(Ok(list)) => execute_sql_query(list, &query).await,
                    Some(Err(err)) => Err(err.clone()),
                    None => fetch_error
                        .as_ref()
                        .cloned()
                        .map(Err)
                        .unwrap_or_else(|| Err("No data available yet".to_string())),
                };
                sql_result.set(Some(result));
            });
        })
    };

    let run_sql_click = {
        let run_sql = run_sql.clone();
        Callback::from(move |_: MouseEvent| run_sql.emit(()))
    };

    let on_sql_keydown = {
        let sql_query = sql_query.clone();
        let run_sql = run_sql.clone();
        Callback::from(move |event: KeyboardEvent| {
            let key = event.key();
            if key == "Tab" {
                if let Some(textarea) = event
                    .target()
                    .and_then(|target| target.dyn_into::<HtmlTextAreaElement>().ok())
                {
                    event.prevent_default();
                    let value = textarea.value();
                    let start = textarea.selection_start().ok().flatten().unwrap_or(0) as usize;
                    let end = textarea
                        .selection_end()
                        .ok()
                        .flatten()
                        .unwrap_or(start as u32) as usize;
                    let mut next = String::with_capacity(value.len() + 1);
                    next.push_str(&value[..start]);
                    next.push('\t');
                    next.push_str(&value[end..]);
                    sql_query.set(next);
                    let cursor = (start + 1) as u32;
                    let _ = textarea.set_selection_range(cursor, cursor);
                }
                return;
            }

            if key == "Enter" && (event.shift_key() || event.ctrl_key() || event.meta_key()) {
                event.prevent_default();
                run_sql.emit(());
            }
        })
    };

    {
        let data = repolist.clone();
        let active_query_ref = active_query_ref.clone();
        let sql_result = sql_result.clone();
        let fetch_error_state = fetch_error.clone();
        // Fetch immediately, then refresh every second
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local({
                let data = data.clone();
                let active_query_ref = active_query_ref.clone();
                let sql_result = sql_result.clone();
                let fetch_error = fetch_error_state.clone();
                async move {
                    let res = fetch_repos().await;
                    console::log_1(&"Initial fetch".into());
                    match res {
                        Ok(list) => {
                            fetch_error.set(None);
                            let list = Arc::new(list);
                            data.set(Some(Ok(Arc::clone(&list))));
                            let query = active_query_ref.borrow().clone();
                            console::log_1(&format!("auto_sql: {}", query).into());
                            let sql_result = sql_result.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                let result = execute_sql_query(&list, &query).await;
                                sql_result.set(Some(result));
                            });
                        }
                        Err(err) => {
                            fetch_error.set(Some(err));
                        }
                    }
                }
            });

            let interval = Interval::new(1000, move || {
                let data = data.clone();
                let active_query_ref = active_query_ref.clone();
                let sql_result = sql_result.clone();
                let fetch_error = fetch_error_state.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let res = fetch_repos().await;
                    match res {
                        Ok(list) => {
                            console::log_1(&"Refresh fetch".into());
                            fetch_error.set(None);
                            let list = Arc::new(list);
                            data.set(Some(Ok(Arc::clone(&list))));
                            let query = active_query_ref.borrow().clone();
                            console::log_1(&format!("auto_sql: {}", query).into());
                            let sql_result = sql_result.clone();
                            wasm_bindgen_futures::spawn_local(async move {
                                let result = execute_sql_query(&list, &query).await;
                                sql_result.set(Some(result));
                            });
                        }
                        Err(err) => {
                            fetch_error.set(Some(err));
                        }
                    }
                });
            });

            move || drop(interval)
        });
    }

    let sql_table = html! {
        <section class="card">
            <div style="display: flex; flex-direction: column; gap: 12px;">
                <details class="sql-details">
                    <summary class="sql-summary">{ "SQL query" }</summary>
                    <div class="sql-panel">
                        <label class="meta" for="sql-query">{ "SQL query for package list" }</label>
                        <div class="sql-editor">
                            <pre ref={sql_pre_ref} class="sql-highlight">{ for highlight_sql(&*sql_query) }</pre>
                            <textarea
                                id="sql-query"
                                rows={5}
                                class="sql-input"
                                value={(*sql_query).clone()}
                                oninput={on_sql_input}
                                onscroll={on_sql_scroll}
                                onkeydown={on_sql_keydown}
                            />
                        </div>
                        {
                            if let Some(err) = &*fetch_error {
                                html! { <p class="meta error">{ format!("Fetch error: {}", err) }</p> }
                            } else {
                                html! {}
                            }
                        }
                        <button onclick={run_sql_click} class="sql-run">{ "Run SQL" }</button>
                    </div>
                </details>
                {
                    match &*sql_result {
                        None => html! { <p class="meta">{ "Run the query to display results." }</p> },
                        Some(Ok(result)) => html! {
                            <div style="overflow-x: auto;">
                                <table class="sql-results-table" style="border-collapse: collapse; margin-top: 12px; min-width: max-content;">
                                    <thead>
                                        <tr style="background: var(--card-strong);">
                                            { for result.columns.iter().map(|column| html! {
                                                <th style="padding: 10px 12px; text-align: left; color: var(--accent); white-space: nowrap;">{ column }</th>
                                            }) }
                                        </tr>
                                    </thead>
                                    <tbody>
                                        { for result.rows.iter().map(|row| html! {
                                            <tr style="border-bottom: 1px solid rgba(255, 255, 255, 0.08);">
                                                { for result.columns.iter().enumerate().map(|(idx, column)| {
                                                    let value = row.get(idx).map(|s| s.as_str()).unwrap_or("");
                                                    render_sql_value(column, value, &copy_handler, &*copy_token)
                                                }) }
                                            </tr>
                                        }) }
                                    </tbody>
                                </table>
                            </div>
                        },
                        Some(Err(error)) => html! { <p class="meta error">{ error }</p> },
                    }
                }
            </div>
        </section>
    };

    html! {
        <div class="app-bg">
            <main class="page">
                <header class="page-header">
                    <p class="kicker">{ "Nix Autobuild" }</p>
                </header>
                { sql_table }
            </main>
        </div>
    }
}

//fn main() {
//    yew::Renderer::<App>::new().render();
//}

pub fn main() {
    yew::Renderer::<App>::new().render();
}
