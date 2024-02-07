FROM debian:12.4

RUN mkdir /app
RUN mkdir -p /var/raptor/html
RUN apt update && apt install -y openssl ca-certificates

COPY ./raptor /usr/bin/raptor
COPY default.yml /etc/raptor/default.yml

EXPOSE 8000

WORKDIR /app
CMD raptor -c /etc/raptor/default.yml -l info
