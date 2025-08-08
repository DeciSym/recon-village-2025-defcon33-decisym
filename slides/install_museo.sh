#!/bin/sh

cp museo.zip /tmp
cd /tmp || exit
unzip museo.zip
mkdir -p ~/.local/share/fonts/museo
cp ./*.otf ~/.local/share/fonts/museo/
fc-cache -fv
