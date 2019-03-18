#![feature(await_macro, async_await, futures_api)]
use {
    log::{info, debug, error},
    hyper::{
        // Miscellaneous types from Hyper for working with HTTP.
        Body, Client, Request, Response, Server, Uri, StatusCode, Method,

        // This function turns a closure which returns a future into an
        // implementation of the the Hyper `Service` trait, which is an
        // asynchronous function from a generic `Request` to a `Response`.
        service::service_fn,
    },
    futures::{
        // Extension traits providing additional methods on futures.
        // `FutureExt` adds methods that work for all futures, whereas
        // `TryFutureExt` adds methods to futures that return `Result` types.
        future::{FutureExt, TryFutureExt},
    },
    std::net::SocketAddr,

    tokio::{
        // This is the redefinition of the await! macro which supports both
        // futures 0.1 (used by Hyper and Tokio) and futures 0.3 (the new API
        // exposed by `std::future` and implemented by `async fn` syntax).
        await,
        fs::file::File,
    },
    std::{
        path::{Path, PathBuf},
    },
    mime_guess::get_mime_type_str,
};

mod byte_stream;

const STATIC_FILES: &'static str = "/www";

async fn serve_req(req: Request<Body>, mut root: PathBuf) -> Result<Response<Body>, hyper::Error> {
    info!("REQ {} {}", req.method(), req.uri());
    if req.method() == Method::GET {
        let path = req.uri().path();
        match path {
            filename => {
                root.push(&filename[1..]); // remove leading /
                let extension = &(Path::new(filename).extension().and_then(|s| s.to_str()));
                debug!("Requesting file {:?}", root.to_str());
                match await!(File::open(root.into_boxed_path())) {
                    Ok(file) => {
                        let mut response = Response::builder();
                        if let Some(mimetype) = extension.and_then(|ref extension| get_mime_type_str(&extension)) {
                            response.header("Content-Type", mimetype);
                        }
                        return Ok(response.body(Body::wrap_stream(byte_stream::ByteStream(file))).unwrap());
                    },
                    Err(err) => {
                        error!("{}", err);
                        return Ok(Response::builder().status(StatusCode::NOT_FOUND).body(Body::from("Not Found")).unwrap());
                    }
                }
            },
        }
    }

    Ok(Response::builder().status(StatusCode::METHOD_NOT_ALLOWED).body(Body::from("Only GET Allowed!")).unwrap())
}

async fn run_server(addr: SocketAddr) {
    info!("Listening on http://{}", addr);

    // Create a server bound on the provided address
    let serve_future = Server::bind(&addr)
        // Serve requests using our `async serve_req` function.
        // `serve` takes a closure which returns a type implementing the
        // `Service` trait. `service_fn` returns a value implementing the
        // `Service` trait, and accepts a closure which goes from request
        // to a future of the response. In order to use our `serve_req`
        // function with Hyper, we have to box it and put it in a compatability
        // wrapper to go from a futures 0.3 future (the kind returned by
        // `async fn`) to a futures 0.1 future (the kind used by Hyper).
        .serve(|| service_fn(|req| serve_req(req, PathBuf::from(STATIC_FILES)).boxed().compat()));

    // Wait for the server to complete serving or exit with an error.
    // If an error occurred, print it to stderr.
    if let Err(e) = await!(serve_future) {
        error!("server error: {}", e);
    }
}

fn main() {
    env_logger::init();

    let addr = "127.0.0.1:8080".parse().unwrap();

    tokio::run_async(run_server(addr));
}
