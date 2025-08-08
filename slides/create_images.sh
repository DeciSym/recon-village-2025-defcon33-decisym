#!/bin/sh

batcat \
    --color=always \
    -l yaml \
    ../tests/configs/generate_foaf_rdf.yaml | convert text:- images/generate_foaf_rdf.png
