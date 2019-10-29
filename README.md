Debcrafter
==========

Crafts complex debian packages from *very rich* specification files

About
-----

**Warning: debcrafter is huge WIP! Stuff will definitelly change, but if you're willing to join adventure, PRs are open. This thing is not well-tested yet!**

If you ever attempted to build Debian packages that work together very well, you probably found out it's a tedious task.

Here's an incomplete list of things that you need to handle:

* Place binaries in the correct places
* Place libraries in the correct places
* Figure out dependencies
* If your package is a service:
	* Make sure user is created (if needed)
	* Make sure to execute it with corrrect arguments
	* Make sure to reload the service when configuration is changed (debconf, trigger)
	* Validate the configuration 
	* Copy the configuration from debconf to configuration file (with correct quoting/escaping
	* Set correct permissions and ownership for files

This is probably just a tip of the iceberg. If you plan to package tens of packages, this is crazy.
So I wrote debcrafter.

What debcrafter currently does
------------------------------

Generated files

- [x] config
- [x] postinst
- [x] service
- [x] templates
- [x] triggers
- [x] rules
- [x] control
- [ ] preinst (is it needed?)
- [ ] prerm (is it needed?)
- [x] postrm
- [x] compat
- [x] install
- [ ] dirs
- [ ] desktop
- [ ] conffiles (not needed actually thanks to DH)

Features

- [x] Process service packages
- [x] Copy debconf configuration to plain `key=value` files as well as Toml.
- [x] Set correct ownership and permission for config files and diretories configured from debconf
- [x] Concat configuration for stuff that doesn't support conf dirs
- [x] Configuration extension packages - packages that somehow modify configuration of another package
- [ ] Binary packages (not sure if actually useful)
- [ ] Library packages
- [ ] Generate source package
- [ ] Input validation in config script
- [ ] Upload to a repository
- [ ] Multiple variants of a package (think `mainnet`, `testnet` and `regtest` for Bitcoin)
- [ ] Multiple instances of the same service (think multiple LN wallets)
- [ ] Nicer configuration UI capable of taking full advantage of specification
- [ ] Download sources
- [ ] Autoconf
- [ ] Cargo
- [ ] Go
- [ ] Scala
- [ ] Python
- [ ] Debhelper integration
- [ ] Debconf integration
- [ ] Delegate maintainer scripts to a common specialized program for faster, more reliable and more easily auditable execution

Package specification
---------------------

Currently there are two kinds of packages: `Service` and `ConfExt`. Service packages make sure to configure a system-wide systemd service.

ConfExt packages extend the configuration of services. They are used in cases when one *or more* packages need another package to be configured in a certain way in order to work. While this might be achieved with special scripts, dependencies describe the situation more cleanly.

Example of `Service` package specification:

```toml
# Name of the package
name = "bitcoin-mainnet"
# Name of the package containing binaries
bin_package = "bitcoind"
# Binary that has to be executed
binary = "/usr/bin/bitcoind"
# Parameter used to pass the path to configuration to the binary
conf_param = "-conf"
# Information about user under which the service should run
# You can notice that user name isn't specified - it takes the name
# from the name of the package by default. Can be overriden by name = "...".
# Group always has the same name as user or it's nogroup.
# If create is specified, the user will be created in postinst script.
# home = true asks to create home directory
user = { group = true, create = { home = true } }
# Short information about the package
summary = "Bitcoin fully validating node"
# Extra configuration added to .service file
extra_service_config = """
# Stopping bitcoind can take a very long time
TimeoutStopSec=300
Restart=always
"""

# For each configuration file, there's specification like this one
# This is describing a file that will be in /etc/bitcoin-mainnet/bitcoin.conf
[config."bitcoin.conf"]
# Plain format means key=value pairs without any escaping.
# The other supported format right now is toml which also escapes strings.
format = "plain"
# Tells to concat files in directory relative to /etc/bitcoin-mainnet/ and
# append them to this file.
cat_dir = "conf.d"
# Tells to append these specific files to the config file
cat_files = ["chain_mode"]

# ivars means internal variables, name of the variable follows
[config."bitcoin.conf".ivars.datadir]
# Type of the variable. Path variables can be created during postinst.
type = "path"
# File type that should be created
file_type = "dir"
# Says to create the file with these attributes
# $service means use the user/group of the service
# BUG: This doesn't currently work, IDK why, the code is there.
create = { mode = 755, owner = "$service", group = "$service" }
# The default value for the variable
default = "/var/lib/bitcoin-mainnet"
# Debconf priority, in this case it's calculated by the script setting PRIORITY variable
priority = { dynamic = { script = "test `df /var | tail -1 | awk '{ print $4; }'` -lt 10000000 && PRIORITY=high || PRIORITY=medium"} }
# Short description of the variable
summary = "Directory containing the timechain data"
# Long description of the variable
long_doc = """
The full path to the directory which will contain timechain data (blocks and chainstate).
Important: you need around 10GB of free space!
"""

[config."bitcoin.conf".ivars.rpcport]
# When input validation is implemented this type will make sure to allow 1..65536
type = "bind_port"
default = "8333"
priority = "low"
summary = "Bitcoin RPC port"

[config."bitcoin.conf".ivars.rpcuser]
type = "string"
default = "bitcoinrpc"
priority = "low"
summary = "Bitcoin RPC port"

[config."bitcoin.conf".ivars.dbcache]
type = "uint"
default = "450"
priority = "medium"
summary = "Size of database cache in MB"

# hvars means hidden variable. This means it's not in debconf but will be added
# to the config file.
[config."bitcoin.conf".hvars.rpcpassword]
type = "string"
# The value of the variable will be calculated with the specified script.
script = "head -c 18 /dev/urandom | base64"
```

The package generated from specification above will have the appropriate .service file created, configuration file placed at correct place, triggers and dependencies generated... If there's anything incorrect (inconsistent) about the resulting package, please file an issue.

Configuration extensions are a little bit simpler:

```toml
name = "bitcoin-fullchain-mainnet"
# Tells which package is extended by this one
extends = "bitcoin-mainnet"
# Tells that some file(s) of extended package is (are)
# replaced by this package
replaces = true
summary = "Bitcoin fully validating node"

[config."chain_mode"]
content = """
prune=0
txindex=0"""
```

This package will place the specified configuraton file where needed to override the configuration.

Apart from package specifications, there's also repository specification file which is very simple.
Example:

```toml
# Used for all in control file
maintainer = "Martin Habostiak <martin.habovstiak@gmail.com>"

# Each source defines a different source directory
[sources.bitcoin]
# Upstream version
version = "0.18.1"
section = "net"
# Packages available within the source
# .sps files must be placed in the same directory this file is in
packages = ["bitcoin-mainnet", "bitcoin-fullchain-mainnet", "bitcoin-txindex-mainnet", "bitcoin-zmq-mainnet"]

[sources.btc-rpc-proxy]
version = "0.1.0"
section = "net"
packages = ["bitcoin-rpc-proxy-mainnet", "bitcoin-timechain-mainnet"]

[sources.electrs]
version = "0.7.0"
section = "net"
packages = ["electrs-mainnet"]
```

Building and usage
------------------

You need [Rust](https://rust-lang.org) in order to build `debcrafter`. (If you wonder why Rust, it's because of its awesome [`serde`](https://crates.io/crates/serde) crate.)

Just download (clone) the source code and run `cargo install --bin gen_deb_repository --path .`

Once you write your repository/package specifications, run `gen_deb_repository /path/to/packages.srs /output/dir` to generate the directory.
You can place `source.changelog` files into source directory too and it will copy those files where appropriate. You can then run `dpkg-buildpackage` from the output
subdirectories.

Plans for the far future
------------------------

There are several things I wish to do quite differently in the future. First and foremost, I'd like to create a new binary that will execute the actions needed in postinst directy from specification file instead of generating postinst script. This will make the packages considerably easier to audit (only need to audit that tool).

Another thing I wish to do is deterministic builds. Currently, I want to have something working, but all packages should be easily auditable at some point. Deterministic builds are crucial part of auditability.

Finally, I realized that non-Debian packages could be created from these specification files. So maybe one day in the future, this tool will create RPM, homebrew and other stuff too.

License
-------

MITNFA
