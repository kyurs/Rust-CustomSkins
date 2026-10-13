#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
: "${JAVA_HOME:?Use JDK 25}"
: "${VELOCITY_JAR:?Path to deployed Velocity jar}"
: "${EAGLER_JAR:?Path to deployed EaglerXServer jar}"
mkdir -p build dist
"$JAVA_HOME/bin/javac" -proc:none -cp "$VELOCITY_JAR:$EAGLER_JAR" -d build src/customskins/*.java tests/customskins/*.java
cp resources/* build/
"$JAVA_HOME/bin/java" -ea -cp "build:$VELOCITY_JAR:$EAGLER_JAR" customskins.SkinTests
"$JAVA_HOME/bin/jar" --create --file dist/custom-skins.jar -C build customskins/CustomSkins.class -C build 'customskins/CustomSkins$SkinCommand.class' -C build customskins/SkinData.class -C build 'customskins/SkinData$Skin.class' -C build 'customskins/SkinData$Choice.class' -C build 'customskins/SkinData$Cached.class' -C build velocity-plugin.json -C build defaults.json
