#!/bin/sh

cp Atkinson-Hyperlegible-Font-Print-and-Web-Final.zip /tmp
cd /tmp || exit
unzip Atkinson-Hyperlegible-Font-Print-and-Web-Final.zip
mkdir -p ~/.local/share/fonts/atkinson-hyperlegible
cp "Atkinson-Hyperlegible-Font-Print-and-Web-2020-0514/Print Fonts"/*.otf ~/.local/share/fonts/atkinson-hyperlegible/
fc-cache -fv
