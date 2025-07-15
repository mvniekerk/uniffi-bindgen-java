#!/bin/bash

cargo build --release
TARGET_TRIPLE=$(rustc --print host-tuple)

case "$OSTYPE" in
  linux*)   CDYLIB_EXT="so" ;;
  darwin*)  CDYLIB_EXT="dylib" ;;
  msys*)    CDYLIB_EXT="dll" ;;
  cygwin*)  CDYLIB_EXT="dll" ;;
  *)        echo "Unknown OS: $OSTYPE"; exit 1 ;;
esac

CDYLIB_PATH="target/release/libsay_true.$CDYLIB_EXT"

JAVA_LIB_DIR="../java"
JAVA_OUT_DIR="$JAVA_LIB_DIR/src/main/java"
JAVA_RESOURCES_DIR="$JAVA_LIB_DIR/src/main/resources"

mkdir -p "$JAVA_OUT_DIR"

if ! command -v uniffi-bindgen-java &> /dev/null
then
    echo "uniffi-bindgen-java could not be found, installing..."
#    --path <> can be replaced by --git <git_repo>
    cargo install uniffi-bindgen-java --path ../../../../../
    if [ $? -ne 0 ]; then
        echo "Error: Failed to install uniffi-bindgen-java."
        exit 1
    fi
fi

uniffi-bindgen-java generate \
    --library "$CDYLIB_PATH" \
    --out-dir "$JAVA_OUT_DIR"

if [ $? -ne 0 ]; then
    echo "Error: uniffi-bindgen-java failed."
    exit 1
fi

echo "Java bindings generated in: $JAVA_OUT_DIR"

mkdir -p "$JAVA_RESOURCES_DIR"
cp $CDYLIB_PATH $JAVA_RESOURCES_DIR/
