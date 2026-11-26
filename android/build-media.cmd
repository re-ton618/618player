@echo off
rem Builds android\media.jar (nsh.json `android.libraries`) from android\java:
rem the media session, its notification and the playback service.
rem Run it after changing the Java; `nsh package` turns the jar into dex.
setlocal
if not defined JAVA_HOME set "JAVA_HOME=C:\Program Files\Android\Android Studio\jbr"
if not defined ANDROID_HOME set "ANDROID_HOME=%LOCALAPPDATA%\Android\Sdk"
set "PLATFORM=%ANDROID_HOME%\platforms\android-35\android.jar"
set "HERE=%~dp0"
set "CLASSES=%HERE%build\classes"

if exist "%HERE%build" rmdir /s /q "%HERE%build"
mkdir "%CLASSES%" || exit /b 1
dir /s /b "%HERE%java\*.java" > "%HERE%build\sources.txt"
"%JAVA_HOME%\bin\javac" --release 11 -Xlint:-options -classpath "%PLATFORM%" -d "%CLASSES%" @"%HERE%build\sources.txt" || exit /b 1
"%JAVA_HOME%\bin\jar" --create --file "%HERE%media.jar" -C "%CLASSES%" . || exit /b 1
echo Built %HERE%media.jar
