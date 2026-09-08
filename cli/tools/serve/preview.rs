//! Development-only HTTP responses; generated files remain deployable and unchanged.

use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use warp::{Filter, Reply};

pub(super) type Revision = Arc<AtomicU64>;

pub(super) fn routes(
    root: PathBuf,
    revision: Option<Revision>,
) -> warp::filters::BoxedFilter<(warp::reply::Response,)> {
    let watching = revision.is_some();
    let files = warp::fs::dir(root)
        .and_then(move |file: warp::fs::File| async move {
            let path = file.path().to_owned();
            let response = file.into_response();
            if watching
                && response.status() == warp::http::StatusCode::OK
                && path
                    .extension()
                    .is_some_and(|extension| extension == "html")
            {
                let html = tokio::fs::read_to_string(path)
                    .await
                    .map_err(|_| warp::reject::not_found())?;
                return Ok::<_, warp::Rejection>(
                    warp::reply::html(inject_reload(&html)).into_response(),
                );
            }
            Ok(response)
        })
        .boxed();
    let routes = if let Some(revision) = revision {
        let script = warp::path!("__berlin" / "live.js")
            .and(warp::get())
            .map(|| {
                warp::reply::with_header(
                    include_str!("live.js"),
                    "content-type",
                    "text/javascript; charset=utf-8",
                )
                .into_response()
            });
        let version = warp::path!("__berlin" / "revision")
            .and(warp::get())
            .map(move || revision.load(Ordering::Relaxed).to_string().into_response());
        script.or(version).unify().or(files).unify().boxed()
    } else {
        files
    };
    routes
        .map(|mut response: warp::reply::Response| {
            response
                .headers_mut()
                .insert("cache-control", "no-store".parse().unwrap());
            response
                .headers_mut()
                .insert("x-robots-tag", "noindex, nofollow".parse().unwrap());
            response
        })
        .boxed()
}

fn inject_reload(html: &str) -> String {
    const SCRIPT: &str = "<script src=\"/__berlin/live.js\" defer></script>";
    // Templates need no development condition and no fixed host or port.
    match html.to_ascii_lowercase().rfind("</body>") {
        Some(position) => format!("{}{SCRIPT}{}", &html[..position], &html[position..]),
        None => format!("{html}{SCRIPT}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reload_is_served_only_when_watching_and_never_written() {
        let root = tempfile::tempdir().unwrap();
        let html = "<html><body>Example</body></html>";
        std::fs::write(root.path().join("index.html"), html).unwrap();
        let revision = Arc::new(AtomicU64::new(1));
        let watched = routes(root.path().to_owned(), Some(revision.clone()));
        let response = warp::test::request().path("/").reply(&watched).await;
        assert_eq!(response.status(), 200);
        assert!(
            std::str::from_utf8(response.body())
                .unwrap()
                .contains("/__berlin/live.js")
        );
        assert_eq!(response.headers()["x-robots-tag"], "noindex, nofollow");
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            std::fs::read_to_string(root.path().join("index.html")).unwrap(),
            html
        );
        assert!(!root.path().join("__berlin").exists());
        revision.store(2, Ordering::Relaxed);
        assert_eq!(
            warp::test::request()
                .path("/__berlin/revision")
                .reply(&watched)
                .await
                .body(),
            "2"
        );
        assert_eq!(
            warp::test::request()
                .path("/__berlin/live.js")
                .reply(&watched)
                .await
                .status(),
            200
        );
        let plain = routes(root.path().to_owned(), None);
        assert_eq!(
            warp::test::request().path("/").reply(&plain).await.body(),
            html
        );
        assert_eq!(
            warp::test::request()
                .path("/__berlin/live.js")
                .reply(&plain)
                .await
                .status(),
            404
        );
    }
}
