#!/bin/bash

echo -n "update core " > core-commit.txt

# update stacks-blockchain from master
cd ../../blockstack/stacks-blockchain
git checkout master
git pull origin master
git rev-parse --short HEAD >> ../../lgalabru/clarity-repl/core-commit.txt

# copy vm directory
cd ../../lgalabru/clarity-repl
rm -rf src/clarity
cp -r ../../blockstack/stacks-blockchain/src/vm src/clarity

# update crate bindings
cd src/clarity
grep -rl 'use vm::' . | xargs sed -i 's/use vm::/use crate::clarity::/g'
cd ../..

# commit
git add .
git commit -F core-commit.txt


