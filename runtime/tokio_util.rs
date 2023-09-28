use libs::futures::future::LocalBoxFuture;
use libs::tokio;
use libs::warp;

pub fn create_basic_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .max_blocking_threads(16)
        .build()
        .unwrap()
}

pub fn run_local<F, R>(future: F) -> R
where
    F: std::future::Future<Output = R>,
{
    let rt = create_basic_runtime();
    let local = tokio::task::LocalSet::new();
    local.block_on(&rt, future)
}

pub fn start_server(
    dir: String,
    ip_addr: std::net::IpAddr,
    port: u16,
) -> LocalBoxFuture<'static, ()> {
    println!("Starting server at http://localhost:{}", port);
    let socket = std::net::SocketAddr::new(ip_addr, port);
    Box::pin(warp::serve(warp::fs::dir(dir)).run(socket))
}
