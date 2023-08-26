# Raptor

Raptor is an easy to use HTTP/2 server.

![Raptor logo](images/raptor_logo.png)

## Getting raptor

For now, you must build raptor yourself using `cargo`. If you don't have rust
installed on your machine you can use [rustup](https://rustup.rs/) to get it.

To get a release build of raptor you just have to run `cargo build`. Omit the
`--release` flag if you need a debug build.

```sh
$ cargo build --release
```

## Configuration

Raptor aims to be easy to use and use [YAML](https://fr.wikipedia.org/wiki/YAML)
as configuration markup language. No more obscure configuration file syntax.

Here is an example configuration file using `localhost` for the inpatients:

```yaml
ip: "127.0.0.1"
port: 8000
cert_path: "cert.pem"
key_path: "private_key.pem"
root_dir: /srv
```

| Name         | Description                                                            | Mandatory | Default value |
|--------------|------------------------------------------------------------------------|-----------|---------------|
| ip           | Server ip (can be `0.0.0.0` for all interfaces)                        | Yes       |               |
| port         | Port to listen (must be a 16 bits unsigned integer)                    | Yes       |               |
| cert_path    | Path to a X509 certificate                                             | Yes       |               |
| key_path     | Path to a RSA private key                                              | Yes       |               |
| root_dir     | An absolute path to the directory from where your files will be served | Yes       |               |
| default_file | Default file to use                                                    | No        | index.html    |

## Known limitations

This server is not really production ready yet and may have to non standard
behaviors. Plain text HTTP/2 is not supported and never will because because
majors browsers like `Firefox` and `Chromium` based browsers don't.

## Planned features to add

- [ ] Support for vhosts (optional in configuration files)
- [ ] Make the server fully RFC compliant (pass all [h2spec](https://github.com/summerwind/h2spec) tests)
- [ ] Support non encrypted connections upgrade (when port 80 is used which is the default for most browsers)
- [ ] Support for HTTP/1.1 as a fallback for clients which doesn't support HTTP/2
