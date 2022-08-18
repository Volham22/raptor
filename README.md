# Raptor Web server

Raptor is a HTTP server written in Rust with aims to use as little memory as
possible and an easy configuration. It is built on top of [tokio](https://tokio.rs/)
an asynchronous runtime for rust.

## Install

There's not prebuilt binaries yet. So it is required to build the project using
the rust tool chain (use [rustup](https://rustup.rs/) to install it if you
don't have the tool chain already installed). Then use cargo to build the project
for you.

``` sh
$ cargo build --release
```

## Usage

In order to launch the server you must provide a `json` config file.
You can check if your custom config is valid with the `--validate` flag.

### Configuration
Here is an sample config file

``` json
{
    "vhosts": [
        {
            "name": "*:8080",
            "ip": "127.0.0.1",
            "port": 8080,
            "root_dir": "."
        },
        {
            "name": "127.0.0.1:3000",
            "ip": "127.0.0.1",
            "port": 3000,
            "is_ipv6": false,
            "root_dir": ".",
            "cert_key": "public.pem",
            "private_key": "private.pem"
        }
    ]
}
```

An virtual host must at least have :
- An ip
- A port
- The root directory of the served files

### Vhost matching

You can use shell pattern matching as the `fnmatch` function does. Not that the
first matching vhost will be used. They're tested in the same order as they're
declared in the config file.

A default vhost can be specified with `is_default` flag, thus it will be used if
a given hostname don't match any existing vhost. If there's no default specified
vhost the first one will be used as a default vhost.

``` json
{
    "vhosts": [
        {
            "name": "127.0.0.1:3000",
            "ip": "127.0.0.1",
            "port": 3000,
            "root_dir": ".",
            "is_default": true
        }
    ]
}
```

### Ipv6 Support
Raptor supports `ipv6`, you can specify an `ipv6` address in the `ip` field.
Don't forget to enable `ipv6` for the vhost using the `is_ipv6` flag.

Here is an example to create a vhost listening on port `8080` for `localhost`.

``` json
{
    "vhosts": [
        {
            "name": "[::1]:8080",
            "ip": "::1",
            "port": 8080,
            "root_dir": "."
        }
    ]
}
```


### Tls support
Raptor supports tls connections (HTTPS). A vhost will use a secure connection
if both `cert_key` and `private_key` are specified otherwise it will fallback to
plain HTTP.

Example:

``` json
{
    "vhosts": [
        {
            "name": "127.0.0.1:3000",
            "ip": "127.0.0.1",
            "port": 3000,
            "root_dir": ".",
            "cert_key": "public.pem",
            "private_key": "private.pem"
        }
    ]
}
```

## Note

This server is still in development and is therefore not production ready.

## TODOLIST

- Complete file management with HTTP method like `PUT` and `PATCH`.
- Support for compression
- More flags to control how the server behave

