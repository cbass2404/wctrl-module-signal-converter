@echo off
setlocal enabledelayedexpansion

rem  tools\release.cmd - tag and push a release. Maintainer tool.
rem
rem  Order of checks:
rem    1. on main, and main matches origin - releases build only from main,
rem       and main takes changes through pull requests, so nothing is pushed
rem       but the tag. Uncommitted changes warn and ask.
rem    2. tag from VERSION.md must not already exist, locally or on origin
rem    3. the checks the pipeline would fail on: the manifests agree with
rem       VERSION.md, and data\nightly-only.json is current
rem    4. asks for a release message; submitting an empty one cancels
rem    5. pushes the tag
rem
rem  Pushing the tag is what triggers .github/workflows/release.yml, which
rem  checks the tag again, runs the tests, builds and drafts the release. The
rem  tests run there, not here, so they cannot be skipped.

rem Everything below runs against the repo root, one level up from tools\.
cd /d "%~dp0.."

echo(
echo ===============================================
echo   Release
echo ===============================================
echo(

rem --- must be in a git repo ----------------------------------------------
git rev-parse --is-inside-work-tree >nul 2>&1
if errorlevel 1 (
    echo ERROR: not a git repository.
    goto :fail
)

for /f "usebackq tokens=* delims= " %%b in (`git rev-parse --abbrev-ref HEAD`) do set "BRANCH=%%b"
for /f "usebackq tokens=* delims= " %%c in (`git rev-parse --short HEAD`) do set "COMMIT=%%c"

echo   branch : %BRANCH%
echo   commit : %COMMIT%  (this is what the tag will point at)
echo(

rem ==========================================================================
rem  1. on main, in step with origin
rem ==========================================================================

if not "%BRANCH%"=="main" (
    echo ERROR: releases are built only from main, and this is '%BRANCH%'.
    echo(
    echo   Merge the work into main through a pull request, then:
    echo       git switch main
    echo       git pull
    goto :fail
)

rem Refresh remote refs first, or "in step" is judged against stale data.
echo   fetching origin ...
git fetch -q origin >nul 2>&1
if errorlevel 1 (
    echo ERROR: could not fetch origin.
    goto :fail
)
echo(

set "AHEAD=0"
set "BEHIND=0"
for /f "usebackq tokens=* delims= " %%n in (`git rev-list --count "origin/main..HEAD"`) do set "AHEAD=%%n"
for /f "usebackq tokens=* delims= " %%n in (`git rev-list --count "HEAD..origin/main"`) do set "BEHIND=%%n"

if not "!AHEAD!"=="0" (
    echo ERROR: !AHEAD! commit^(s^) here are not on origin/main:
    echo(
    git log --oneline "origin/main..HEAD"
    echo(
    echo   The release workflow refuses a tag whose commit is not on main.
    echo   Get these onto main through a pull request, then pull and re-run.
    goto :fail
)

if not "!BEHIND!"=="0" (
    echo ERROR: origin/main is !BEHIND! commit^(s^) ahead of you.
    echo(
    echo   Pull first, so the tag is on what main actually holds:
    echo       git pull
    goto :fail
)

rem Uncommitted: staged, unstaged or untracked. --porcelain covers all three.
set "DIRTY="
for /f "usebackq delims=" %%s in (`git status --porcelain`) do set "DIRTY=1"

if not defined DIRTY (
    echo   working tree clean, main in step with origin.
    echo(
    goto :state_ok
)

echo   -----------------------------------------------
echo     Heads up
echo   -----------------------------------------------
echo(
echo   Uncommitted changes:
echo(
git status --short
echo(
echo   These will NOT be in the release. A tag captures only what is
echo   committed, so anything above is left out.
echo(
echo   -----------------------------------------------
set "SURE="
set /p "SURE=Continue anyway? (y/N): "
if /i "!SURE!"=="y"   goto :state_ok
if /i "!SURE!"=="yes" goto :state_ok
echo(
echo Cancelled - nothing was tagged or pushed.
goto :end

:state_ok

rem ==========================================================================
rem  2. version and tag availability
rem ==========================================================================

if not exist "VERSION.md" (
    echo ERROR: no VERSION.md in %CD%
    goto :fail
)

set "VERSION="
for /f "usebackq tokens=* delims= " %%v in ("VERSION.md") do (
    if not defined VERSION set "VERSION=%%v"
)
if not defined VERSION (
    echo ERROR: VERSION.md is empty.
    goto :fail
)

rem The release workflow requires exactly v + VERSION.md.
set "TAG=v%VERSION%"

echo   VERSION.md   : %VERSION%
echo   tag to create: %TAG%
echo(

rem Check origin first: whether the tag is PUBLISHED decides the remedy. The
rem fetch above pulls remote tags down, so a released tag also shows up locally
rem and reporting it as merely "local" would suggest deleting it - wrong for a
rem tag other people may already have.
echo   checking origin for an existing %TAG% ...
set "ONORIGIN="
git ls-remote --exit-code --tags origin "refs/tags/%TAG%" >nul 2>&1
if not errorlevel 1 set "ONORIGIN=1"

set "ONLOCAL="
git rev-parse -q --verify "refs/tags/%TAG%" >nul 2>&1
if not errorlevel 1 set "ONLOCAL=1"

if defined ONORIGIN (
    echo(
    echo ===============================================
    echo   ERROR: %TAG% has already been released.
    echo ===============================================
    echo(
    echo   The tag exists on origin, so it may already be published and other
    echo   people may have it. A released tag must not be moved.
    echo(
    echo   Bump VERSION.md to the next version, run  python tools\version.py
    echo   to stamp it, merge that to main and release it instead.
    goto :fail
)

if defined ONLOCAL (
    echo(
    echo ERROR: tag %TAG% exists locally but not on origin.
    echo(
    echo   It was probably created and never pushed, or pushed and then
    echo   deleted from origin. It may point at an older commit than HEAD.
    echo(
    echo   Delete it and re-run to tag the current commit:
    echo       git tag -d %TAG%
    goto :fail
)

echo   ok, %TAG% is free.
echo(

rem ==========================================================================
rem  3. what the pipeline would fail on, caught before a tag exists
rem ==========================================================================

echo   checking the manifests agree with VERSION.md ...
python tools\version.py --check
if errorlevel 1 (
    echo(
    echo ERROR: the manifests are out of step with VERSION.md. Run
    echo     python tools\version.py
    echo   and merge the result to main.
    goto :fail
)
echo(

rem nightly_only.py rewrites data\nightly-only.json from the latest stable
rem DCS-BIOS. The release ships the committed file and fails if this would
rem change it, so a change here has to reach main first.
echo   checking data\nightly-only.json against the latest stable DCS-BIOS ...
python tools\nightly_only.py
if errorlevel 1 (
    echo(
    echo ERROR: tools\nightly_only.py failed.
    goto :fail
)
git diff --quiet -- data/nightly-only.json
if errorlevel 1 (
    echo(
    echo ERROR: data\nightly-only.json is out of date, and has been rewritten:
    echo(
    git diff --stat -- data/nightly-only.json
    echo(
    echo   The release would fail on this. Commit it, merge it to main
    echo   through a pull request, then pull and re-run.
    goto :fail
)
echo   ok, current.
echo(

rem ==========================================================================
rem  4. message doubles as the final confirmation
rem ==========================================================================

echo Enter a release message for %TAG%.
echo Press Enter on an empty line to cancel.
echo(
set "MSG="
set /p "MSG=Message: "

if not defined MSG (
    echo(
    echo Cancelled - nothing was tagged or pushed.
    goto :end
)

rem ==========================================================================
rem  5. tag and push
rem ==========================================================================

echo(
echo Tagging %COMMIT% as %TAG% ...
git tag -a "%TAG%" -m "!MSG!"
if errorlevel 1 (
    echo ERROR: git tag failed.
    goto :fail
)

echo Pushing %TAG% ...
git push origin "%TAG%"
if errorlevel 1 (
    echo(
    echo ERROR: pushing the tag failed. Remove the local tag with:
    echo     git tag -d %TAG%
    goto :fail
)

echo(
echo ===============================================
echo   Pushed %TAG%
echo ===============================================
echo(
for /f "usebackq tokens=* delims= " %%u in (`git remote get-url origin`) do set "ORIGIN=%%u"
set "ORIGIN=%ORIGIN:.git=%"
echo   The release workflow is now running:
echo     %ORIGIN%/actions
echo(
echo   When it goes green, a draft is waiting here to be read and published:
echo     %ORIGIN%/releases
echo(
goto :end

:fail
echo(
set "RC=1"

:end
if not defined RC set "RC=0"
echo(
pause
exit /b %RC%
