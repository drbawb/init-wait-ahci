#!/usr/bin/env bash

set -euo pipefail

mv -ft /usr/local/bin target/debug/init-wait-ahci
chown root:wheel /usr/local/bin/init-wait-ahci
chmod 750 /usr/local/bin/init-wait-ahci

