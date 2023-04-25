<img src="https://raw.githubusercontent.com/eclipse-zenoh/zenoh/master/zenoh-dragon.png" height="150">

<!--- 
[![CI](https://github.com/eclipse-zenoh/zenoh-plugin-dds/workflows/Rust/badge.svg)](https://github.com/eclipse-zenoh/zenoh-plugin-dds/actions?query=workflow%3ARust)
--->
[![Discussion](https://img.shields.io/badge/discussion-on%20github-blue)](https://github.com/eclipse-zenoh/roadmap/discussions)
[![Discord](https://img.shields.io/badge/chat-on%20discord-blue)](https://discord.gg/2GJ958VuHs)
[![License](https://img.shields.io/badge/License-EPL%202.0-blue)](https://choosealicense.com/licenses/epl-2.0/)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

# Eclipse Zenoh
The Eclipse Zenoh: Zero Overhead Pub/sub, Store/Query and Compute.

Zenoh (pronounce _/zeno/_) unifies data in motion, data at rest and computations. It carefully blends traditional pub/sub with geo-distributed storages, queries and computations, while retaining a level of time and space efficiency that is well beyond any of the mainstream stacks.

Check the website [zenoh.io](http://zenoh.io) and the [roadmap](https://github.com/eclipse-zenoh/roadmap) for more detailed information.

-------------------------------
# ROS1 to Zenoh Bridge plugin

## Background
ROS1 is a well-known mature platform for building robotic systems. Despite the fact that next generation of ROS - ROS2 is released long time ago, many developers still prefer using ROS1. In order to integrate ROS1 systems to Zenoh infrastructure, [as it was done for DDS/ROS2](https://github.com/eclipse-zenoh/zenoh-plugin-dds), ROS1 to Zenoh Bridge was designed.

## How to install it
Currently, only out-of-source build is supported.

## How to build it

> :warning: **WARNING** :warning: : Zenoh and its ecosystem are under active development. When you build from git, make sure you also build from git any other Zenoh repository you plan to use (e.g. binding, plugin, backend, etc.). It may happen that some changes in git are not compatible with the most recent packaged Zenoh release (e.g. deb, docker, pip). We put particular effort in mantaining compatibility between the various git repositories in the Zenoh project.

> :warning: **WARNING** :warning: : As Rust doesn't have a stable ABI, the plugins should be
built with the exact same Rust version than `zenohd`, and using for `zenoh` dependency the same version (or commit number) than 'zenohd'.
Otherwise, incompatibilities in memory mapping of shared types between `zenohd` and the library can lead to a `"SIGSEV"` crash.

In order to build the ROS1 to Zenoh Bridge, you need first to install the following dependencies:

- [Rust](https://www.rust-lang.org/tools/install)

Once Rust is in place, you may clone the repository on your machine:

```bash
$ git clone https://github.com/eclipse-zenoh/zenoh-plugin-ros1.git
$ cd zenoh-plugin-ros1
```

Build as a plugin library that can be dynamically loaded by the zenoh router (`zenohd`):
```bash
$ cargo build --release
```
The plugin shared library (`*.so` on Linux, `*.dylib` on Mac OS, `*.dll` on Windows) will be generated in the `target/release` subdirectory.

If you want to run examples or tests, you need to install ROS1:
```bash
$ sudo apt install -y ros-base
```

## Implementation
Currently, ROS1 to Zenoh Bridge is based on [rosrust library fork](https://github.com/ZettaScaleLabs/rosrust). Some limitations are applied due to rosrust's implementation details, and we are targeting to re-engineer rosrust to overcome this

## Limitations
- all topic names are bridged as-is
- all topic datatypes and md5 sums are bridged as "*" wildcard and may not work with some ROS1 client implementations
- there is a performance impact coming from rosrust
