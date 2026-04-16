use crate::backend::{BUILD_REPOS, FRONTEND_PATH};
use actix_web::{HttpResponse, Responder, get};

#[allow(static_mut_refs)]
#[get("/repos")]
pub async fn repos() -> impl Responder {
    println!("INFO\tRequested repo info");
    let json = serde_json::to_string_pretty(unsafe { &BUILD_REPOS }).unwrap();
    // Append Access-Control-Allow-Origin for browsers during development.
    HttpResponse::Ok().body(json)
}

pub async fn server_nix_file(path: String) -> actix_web::Result<HttpResponse> {
    println!("INFO\tRequested nix file: {}", path);

    let metadata = match std::fs::metadata(&path) {
        Ok(meta) => meta,
        Err(_) => return Err(actix_web::error::ErrorNotFound("404 Not Found")),
    };

    if metadata.is_file() {
        match std::fs::read(&path) {
            Ok(contents) => Ok(HttpResponse::Ok().body(contents)),
            Err(_) => Err(actix_web::error::ErrorNotFound("404 Not Found")),
        }
    } else if metadata.is_dir() {
        match std::fs::read_dir(&path) {
            Ok(entries) => {
                let mut listing = String::from("<html><body><h1>Directory listing</h1><ul>");
                for entry in entries.flatten() {
                    if let Ok(name) = entry.file_name().into_string() {
                        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        let suffix = if is_dir { "/" } else { "" };
                        listing.push_str(&format!(
                            "<li><a href=\"{}/{}{}\">{}{}</a></li>",
                            path, name, suffix, name, suffix
                        ));
                    }
                }
                listing.push_str("</ul></body></html>");
                Ok(HttpResponse::Ok().content_type("text/html").body(listing))
            }
            Err(_) => Err(actix_web::error::ErrorInternalServerError(
                "Failed to read directory",
            )),
        }
    } else {
        Err(actix_web::error::ErrorNotFound("404 Not Found"))
    }
}

#[get("/nix/store{path:.*}")]
// serve file if available or list the nix store directory
pub async fn nix_store_files(
    path: actix_web::web::Path<String>,
) -> actix_web::Result<HttpResponse> {
    let full_path = format!("/nix/store{}", path.into_inner());
    server_nix_file(full_path).await
}

#[get("/store{path:.*}")]
// serve file if available or list the nix store directory
pub async fn store_files(path: actix_web::web::Path<String>) -> actix_web::Result<HttpResponse> {
    let full_path = format!("/nix/store{}", path.into_inner());
    server_nix_file(full_path).await
}

#[get("/{path:.*}")]
pub async fn static_files(
    path: actix_web::web::Path<String>,
) -> actix_web::Result<actix_files::NamedFile> {
    let file_path = if path.is_empty() {
        "index.html".to_string()
    } else {
        path.into_inner()
    };
    println!("INFO\tRequested static file: {}", file_path);
    // TODO: Sanitize file_path to prevent directory traversal attacks

    let full_path = format!("{}/{}", FRONTEND_PATH, file_path);
    println!("INFO\tFull static file path: {}", full_path);
    match actix_files::NamedFile::open_async(full_path).await {
        Ok(named_file) => Ok(named_file.use_last_modified(true)),
        Err(_) => Err(actix_web::error::ErrorNotFound("404 Not Found")),
    }
}
