#!/bin/sh
set -eu
# This directory is deliberately NOT in the package payload. dpkg/rpm must never
# own, replace, or remove the user's theme files, even on purge. No removal hook.
mkdir -p /usr/lib/Maploom/themes
