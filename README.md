CachingProxy is a http proxy with the following designed features:

- Cache the response of a request for the first time to speed up repeated requests afterwards.
- Cached websites are offline browsable.
- Configurable caching policies according to page path and query parameters.

### Install

1. Install [rustup](https://rustup.rs/).
2. Run `cargo install --git https://github.com/J-F-Liu/caching_proxy`

### Usage

```
CachingProxy 0.1.0

USAGE:
    caching_proxy.exe [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -c, --cache <cache>      The folder path to store cached files [default: <home_dir>/CachingProxy]
    -l, --listen <listen>    The IP and port of the proxy server to listen on [default: 127.0.0.1:8080]
```
