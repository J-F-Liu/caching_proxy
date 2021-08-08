use directories_next::UserDirs;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Request, Response, Server, StatusCode, Uri};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "CachingProxy")]
struct Args {
    /// The IP:Port of the proxy server to listen on
    #[structopt(short, long, default_value = "127.0.0.1:8080")]
    pub listen: String,

    /// The folder path to store cached files [default: <home_dir>/CachingProxy]
    #[structopt(short, long, parse(from_os_str))]
    pub cache: Option<PathBuf>,
}

static mut CACHE_PATH: String = String::new();

#[tokio::main]
pub async fn main() {
    let args = Args::from_args();

    let cache = if let Some(path) = args.cache {
        path
    } else if let Some(user_dirs) = UserDirs::new() {
        user_dirs.home_dir().join("CachingProxy")
    } else {
        std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    };
    let cache_path = cache.join("Websites");
    unsafe {
        CACHE_PATH = cache_path.to_string_lossy().to_string();
        println!("Cache: {}", &CACHE_PATH);
    }

    let addr = SocketAddr::from_str(&args.listen).unwrap();

    // For every connection, we must make a `Service` to handle all
    // incoming HTTP requests on said connection.
    let make_svc = make_service_fn(|_conn| {
        // This is the `Service` that will handle the connection.
        // `service_fn` is a helper to convert a function that
        // returns a Response into a `Service`.
        async { Ok::<_, Infallible>(service_fn(proxy)) }
    });

    let server = Server::bind(&addr)
        .serve(make_svc)
        .with_graceful_shutdown(shutdown_signal());

    println!("Listening on http://{}", addr);

    // Run this server for... forever!
    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

async fn shutdown_signal() {
    // Wait for the CTRL+C signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}

async fn proxy(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let uri = req.uri();
    println!("{} {}", req.method(), uri.to_string());

    if uri.scheme_str() == Some("http") {
        let file_path = create_file_path(uri);
        if file_path.is_file() {
            println!("Read {}", file_path.to_string_lossy());
            let data = std::fs::read(file_path).unwrap();
            let body = Body::from(data);
            let response = Response::builder().status(200).body(body).unwrap();
            Ok(response)
        } else {
            let client = Client::new();

            match client.request(req).await {
                Ok(response) => {
                    println!("Write {}", file_path.to_string_lossy());
                    let dir = file_path.parent().unwrap();
                    if !dir.is_dir() {
                        std::fs::create_dir_all(dir).unwrap();
                    }
                    let (parts, body) = response.into_parts();
                    let encoding = parts
                        .headers
                        .get(hyper::header::CONTENT_ENCODING)
                        .map(|v| v.to_str().ok())
                        .flatten();
                    let bytes = hyper::body::to_bytes(body).await?;
                    save_file(&file_path, &bytes, encoding);
                    Ok(Response::from_parts(parts, Body::from(bytes)))
                }
                Err(error) => Err(error),
            }
        }
    } else {
        let mut not_found = Response::default();
        *not_found.status_mut() = StatusCode::NOT_FOUND;
        Ok(not_found)
    }
}

fn create_file_path(uri: &Uri) -> PathBuf {
    let mut fullpath = unsafe { PathBuf::from(&CACHE_PATH) };

    let host = uri.host().unwrap();
    fullpath.push(host);

    let path = Path::new(uri.path());
    for component in path.components() {
        if let std::path::Component::Normal(segment) = component {
            fullpath.push(segment);
        }
    }

    if uri.path().ends_with("/") {
        fullpath.push("index.html");
    }

    fullpath
}

fn save_file(file_path: &Path, bytes: &[u8], encoding: Option<&str>) {
    use brotli_decompressor::reader::Decompressor;
    use flate2::read::{DeflateDecoder, GzDecoder};
    use std::io::Read;

    if let Some(encoding) = encoding {
        match encoding {
            "gzip" => {
                let mut decoder = GzDecoder::new(&bytes[..]);
                let mut buffer = Vec::new();
                decoder.read_to_end(&mut buffer).unwrap();
                std::fs::write(file_path, buffer).unwrap();
            }
            "deflate" => {
                let mut deflater = DeflateDecoder::new(&bytes[..]);
                let mut buffer = Vec::new();
                deflater.read_to_end(&mut buffer).unwrap();
                std::fs::write(file_path, buffer).unwrap();
            }
            "br" => {
                let mut decompressor = Decompressor::new(&bytes[..], 4096);
                let mut buffer = Vec::new();
                decompressor.read_to_end(&mut buffer).unwrap();
                std::fs::write(file_path, buffer).unwrap();
            }
            _ => {
                println!("Unknown content encoding: {}", encoding);
                std::fs::write(file_path, bytes).unwrap();
            }
        }
    } else {
        std::fs::write(file_path, bytes).unwrap();
    }
}
