#!/bin/bash

# Step 1: Cross compile for lambdas
cargo lambda build --release --arm64 --output-format zip

# Step 2: Move the config directory to the where the lambda binary has been compiled
cp -r ./config ./target/lambda/main_lambda/config

# Step 3: Replace the base path of configs in the config.yaml to where we mount the network file sustem
# WARNING: THIS SECTION IS A LITTLE HACKY AND UNTESTED ON LINUX ENVIRONMENTS.
if [[ "$OSTYPE" == "darwin"* ]]; then
    # Mac OSX
    sed -i '' 's|./storage|/mnt/efs/storage|g' ./target/lambda/main_lambda/config/config.yaml
    sed -i '' 's|./snapshots|/mnt/efs/snapshots|g' ./target/lambda/main_lambda/config/config.yaml
else
    # Linux
    sed -i 's|./storage|/mnt/efs/storage|g' ./target/lambda/main_lambda/config/config.yaml
    sed -i 's|./snapshots|/mnt/efs/snapshots|g' ./target/lambda/main_lambda/config/config.yaml
fi

# Step 4: Add the modified config folder to the zip archive
cd target/lambda/main_lambda/
zip -ur bootstrap.zip ./config

# Step 5: Remove the copied config directory
rm -r ./config