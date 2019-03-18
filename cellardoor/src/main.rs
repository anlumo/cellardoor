#![feature(await_macro, async_await, futures_api)]
use {
    log::{info, debug, error},
    hyper::{
        // Miscellaneous types from Hyper for working with HTTP.
        Body, Request, Response, Server, StatusCode, Method,

        // This function turns a closure which returns a future into an
        // implementation of the the Hyper `Service` trait, which is an
        // asynchronous function from a generic `Request` to a `Response`.
        service::service_fn,

        header::{HeaderValue, UPGRADE, CONTENT_TYPE, CONNECTION, SEC_WEBSOCKET_VERSION, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_ACCEPT},
        upgrade::Upgraded,
    },
    futures::{
        // Extension traits providing additional methods on futures.
        // `FutureExt` adds methods that work for all futures, whereas
        // `TryFutureExt` adds methods to futures that return `Result` types.
        future::{FutureExt, TryFutureExt},
        stream::StreamExt,
        compat::{Stream01CompatExt, Future01CompatExt},
    },
    std::net::SocketAddr,

    tokio::{
        // This is the redefinition of the await! macro which supports both
        // futures 0.1 (used by Hyper and Tokio) and futures 0.3 (the new API
        // exposed by `std::future` and implemented by `async fn` syntax).
        fs::file::File,
        codec::{Decoder, Framed},
    },
    std::{
        path::{Path, PathBuf},
    },
    mime_guess::get_mime_type_str,
    websocket::{
        r#async::{MessageCodec, MsgCodecCtx},
        message::OwnedMessage,
    },
};

mod byte_stream;

const STATIC_FILES: &'static str = "/www";
const WEBSOCKET_MAGIC: &'static str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

async fn serve_ws(framed: Framed<Upgraded, MessageCodec<OwnedMessage>>) {
    let mut framed = framed.compat();
    while let Some(message) = await!(framed.next()) {
        debug!("Received message: {:?}", message);
    }
}

async fn serve_req(req: Request<Body>, mut root: PathBuf) -> Result<Response<Body>, hyper::Error> {
    info!("REQ {} {}", req.method(), req.uri());
    if req.method() == Method::GET {
        if req.headers().contains_key(UPGRADE) {
            debug!("Upgrade to websocket!");

            if Some(&HeaderValue::from_static("13")) == req.headers().get(SEC_WEBSOCKET_VERSION) {
                if let Some(key) = req.headers().get(SEC_WEBSOCKET_KEY) {
                    let mut hash = sha1::Sha1::new();
                    hash.update(key.as_bytes());
                    hash.update(WEBSOCKET_MAGIC.as_bytes());
                    let accept_str = base64::encode(&hash.digest().bytes());

                    tokio::spawn((async move {
                        if let Ok(upgraded) = await!(req.into_body().on_upgrade().compat()) {
                            await!(serve_ws(MessageCodec::default(MsgCodecCtx::Server).framed(upgraded)));
                        } else {
                            error!("WebSocket upgrade failed.");
                        }
                        Ok(())
                    }).boxed().compat());

                    Ok(Response::builder().status(StatusCode::SWITCHING_PROTOCOLS)
                        .header(UPGRADE, "websocket")
                        .header(CONNECTION, "Upgrade")
                        .header(SEC_WEBSOCKET_ACCEPT, accept_str)
                        .body(Body::from("Switching protocols")).unwrap())
                } else {
                    Ok(Response::builder().status(StatusCode::BAD_REQUEST).body(Body::from("Missing Sec-WebSocket-Key")).unwrap())
                }
            } else {
                Ok(Response::builder().status(StatusCode::BAD_REQUEST).body(Body::from("Unknown WebSocket version")).unwrap())
            }

        } else {
            let filename = req.uri().path();
            root.push(&filename[1..]); // remove leading /
            let extension = &(Path::new(filename).extension().and_then(|s| s.to_str()));
            debug!("Requesting file {:?}", root.to_str());
            match await!(File::open(root.into_boxed_path()).compat()) {
                Ok(file) => {
                    let mut response = Response::builder();
                    if let Some(mimetype) = extension.and_then(|ref extension| get_mime_type_str(&extension)) {
                        response.header(CONTENT_TYPE, mimetype);
                    }
                    Ok(response.body(Body::wrap_stream(byte_stream::ByteStream(file))).unwrap())
                },
                Err(err) => {
                    error!("{}", err);
                    Ok(Response::builder().status(StatusCode::NOT_FOUND).body(Body::from("Not Found")).unwrap())
                }
            }
        }
    } else {
        Ok(Response::builder().status(StatusCode::METHOD_NOT_ALLOWED).body(Body::from("Only GET Allowed!")).unwrap())
    }
}

async fn run_server(addr: SocketAddr) -> Result<(), hyper::error::Error> {
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
    if let Err(e) = await!(serve_future.compat()) {
        error!("server error: {}", e);
        Err(e)
    } else {
        Ok(())
    }
}

fn main() {
    env_logger::init();

    let addr = "127.0.0.1:8080".parse().unwrap();

    tokio::run(run_server(addr).map_err(|e| { error!("{}", e); }).boxed().compat());
}
