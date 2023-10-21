FROM debian:bookworm-slim as runtime

RUN apt-get update
RUN apt-get upgrade -y

RUN apt-get install libssl3 -y
RUN useradd -ms /bin/bash feeder

ARG release_name

COPY ./target/${release_name}/erddap-feeder /usr/local/bin

USER feeder
WORKDIR /home/feeder

ENTRYPOINT ["/usr/local/bin/erddap-feeder"]
