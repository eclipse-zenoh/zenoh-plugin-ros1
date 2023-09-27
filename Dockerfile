#
# Copyright (c) 2022 ZettaScale Technology
#
# This program and the accompanying materials are made available under the
# terms of the Eclipse Public License 2.0 which is available at
# http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
# which is available at https://www.apache.org/licenses/LICENSE-2.0.
#
# SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
#
# Contributors:
#   ZettaScale Zenoh Team, <zenoh@zettascale.tech>
#


###
### Build part
###
FROM rust:slim-buster as builder

WORKDIR /usr/src/zenoh-plugin-ros1

# List of installed tools:
#  * for zenoh-plugin-ros1
#     - git
#     - clang
RUN apt-get update && apt-get -y install git clang build-essential

COPY . .
# if exists, copy .git directory to be used by git-version crate to determine the version
COPY .gi? .git/

RUN cargo install --locked --path zenoh-bridge-ros1


###
### Run part
###
FROM debian:buster-slim

COPY --from=builder /usr/local/cargo/bin/zenoh-bridge-ros1 /usr/local/bin/zenoh-bridge-ros1
RUN ldconfig -v

RUN echo '#!/bin/bash' > /entrypoint.sh
RUN echo 'echo " * Starting: zenoh-bridge-ros1 $*"' >> /entrypoint.sh
RUN echo 'exec zenoh-bridge-ros1 $*' >> /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 7446/udp
EXPOSE 7447/tcp
EXPOSE 8000/tcp

ENV RUST_LOG info

ENTRYPOINT ["/entrypoint.sh"]
