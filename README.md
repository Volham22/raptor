# Raptor

Raptor is an easy to use HTTP/2 server.

<p align="center">
    <img src="https://raw.githubusercontent.com/Volham22/raptor/e185bcbb8b73ff73cd29ffafbe37555a63b540b6/images/raptor_transparent.png" width="50%">
</p>

## Getting raptor

For now, you must build raptor yourself using `cargo`. If you don't have rust
installed on your machine you can use [rustup](https://rustup.rs/) to get it.

To get a release build of raptor you just have to run `cargo build`. Omit the
`--release` flag if you need a debug build.

```sh
$ cargo build --release
```

### Docker

It's possible to run `raptor` using docker. An image is available on
[dockerhub](https://hub.docker.com/r/volham/raptor).

It's possible to run the server using this command:

```sh
docker run -v path/to/keys:/app -v path/to/html:/var/raptor/html volham/raptor
```

You can also override the default configuration located at `/etc/raptor/default.yml`.

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
| log_file     | An absolute path to the file the server should log.                    | No        | Log to stdout |

## Known limitations

This server is not really production ready yet and may have non standard
behaviors. Plain text HTTP/2 is not supported and never will because
majors browsers like `Firefox` and `Chromium` based browsers don't.

## Planned features to add

- [ ] Support for vhosts (optional in configuration files)
- [ ] Make the server fully RFC compliant (pass all [h2spec](https://github.com/summerwind/h2spec) tests)
- [ ] Support non encrypted connections upgrade (when port 80 is used which is the default for most browsers)
- [X] Support for HTTP/1.1 as a fallback for clients which doesn't support HTTP/2
