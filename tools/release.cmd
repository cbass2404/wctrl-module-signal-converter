@echo off
setlocal enabledelayedexpansion

rem  tools\release.cmd - tag and push a release. Maintainer tool.
rem
rem  Order of checks:
rem    0. the shipped profiles and MCDU pages: everything else here can be
rem       checked by the machine, and this cannot. Changes to data\defaults
rem       and data\default-pages are not written up as they land, so this is
rem       the one thing reconstructed from memory.
rem    1. on main, and main matches origin - releases build only from main,
rem       and main takes changes through pull requests, so nothing is pushed
rem       but the tag. Uncommitted changes warn and ask.
rem    2. tag from VERSION.md must not already exist, locally or on origin
rem    3. the checks the pipeline would fail on: every version in the repo
rem       agrees with VERSION.md, in HEAD as well as in the working tree,
rem       data\nightly-only.json is current, and data\defaults-previous and
rem       data\default-pages-previous still hold what the last release shipped
rem    4. CHANGELOG.md has a section for this version, or you say go anyway
rem    5. asks for a release message; submitting an empty one cancels
rem    6. pushes the tag
rem    7. refreshes both snapshots for the NEXT release, and leaves them
rem       unstaged for you to branch and open a pull request from
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

rem ==========================================================================
rem  0. the shipped profiles and pages, named in CHANGELOG.md
rem ==========================================================================
rem
rem Asked first, and asked of a person, because it is the one thing here the
rem machine cannot check. An update never rewrites a lamp row somebody has
rem changed and only corrects a display field still exactly as it shipped, so
rem a fix to a default reaches anybody who has touched that row only if the
rem notes name it and they choose to take it. Changes to data\defaults are
rem deliberately not written up as they land, since they move a great deal
rem through experimentation, which is exactly why this is easy to forget.
rem
rem The shipped MCDU pages are the same: a fix to a page reaches somebody who
rem edited it only if the notes name it, so they are listed beside the profiles.
rem
rem The files that moved are listed, so the answer is not from memory. A
rem previous tag is needed to compare against; without one, the question is
rem asked on its own rather than skipped.

set "LASTTAG="
for /f "usebackq tokens=* delims= " %%t in (`git describe --tags --abbrev^=0 --match "v*" 2^>nul`) do set "LASTTAG=%%t"

if defined LASTTAG (
    echo   Shipped profiles and pages changed since %LASTTAG%:
    echo(
    set "MOVED="
    for /f "usebackq tokens=* delims= " %%f in (`git diff --name-only %LASTTAG% HEAD -- data/defaults data/default-pages 2^>nul`) do (
        set "MOVED=1"
        echo       %%f
    )
    if not defined MOVED echo       none.
) else (
    echo   No previous v* tag to compare against, so the changed profiles
    echo   and pages cannot be listed here.
)
echo(
echo   An update leaves a row or page field you have changed alone, so a fix
echo   to a shipped one reaches those people only if CHANGELOG.md names it.
echo(
echo   -----------------------------------------------
set "NOTED="
set /p "NOTED=Does CHANGELOG.md name every shipped row and page that moved? (y/N): "
if /i "!NOTED!"=="y"   goto :profiles_ok
if /i "!NOTED!"=="yes" goto :profiles_ok
echo(
echo Cancelled - nothing was tagged or pushed.
echo(
echo   Write the changed rows into the '## ' section for this version in
echo   CHANGELOG.md, then run this again.
goto :end

:profiles_ok
echo(

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

rem Read it through version.py, which is what the workflow reads it with, so
rem the tag cannot come out different here over whitespace or a version string
rem that is not one. From HEAD rather than the working tree: the tag names its
rem commit's VERSION.md, and the workflow reads it there.
set "VERSION="
for /f "usebackq tokens=* delims= " %%v in (`python tools\version.py --print --ref HEAD`) do set "VERSION=%%v"
if not defined VERSION (
    echo(
    echo ERROR: could not read a version out of HEAD:VERSION.md.
    goto :fail
)

set "TREE="
for /f "usebackq tokens=* delims= " %%v in (`python tools\version.py --print`) do set "TREE=%%v"

rem Only reachable by continuing past a dirty tree above. The tag would name a
rem version its own commit does not hold, and the workflow rejects that.
if not "!VERSION!"=="!TREE!" (
    echo(
    echo ERROR: VERSION.md says !TREE! here, but HEAD holds !VERSION!.
    echo(
    echo   A tag can only name what its commit holds. Get the bump onto main,
    echo   or put VERSION.md back.
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

rem Every version in the repo, not the few a person thinks to look at:
rem Cargo.toml, Cargo.lock, tauri.conf.json, package.json, package-lock.json,
rem and any workspace crate holding a version of its own. The lock files count
rem - `cargo --locked` and `npm ci` both fail on one that disagrees with its
rem manifest, and they fail deep into the build, long after the tag exists.
rem
rem HEAD, not the working tree, because that is what the tag captures. A stamp
rem that was run but never committed passes a working-tree check and then fails
rem in the pipeline, leaving a tag to delete here and on origin. Catching that
rem here is the point of the whole check.
echo   checking every version in HEAD against VERSION.md ...
echo(
python tools\version.py --check --ref HEAD
if errorlevel 1 (
    echo(
    echo ERROR: HEAD is out of step with VERSION.md. Stamp it and get the
    echo   result onto main before tagging:
    echo       python tools\version.py
    echo       git commit -a
    echo   then open a pull request, merge it, pull and re-run.
    goto :fail
)
echo(

rem The working tree as well, when it differs from HEAD. It is not what gets
rem released, but a half-finished stamp sitting there is how the next release
rem goes wrong, so it is worth saying now.
if not defined DIRTY goto :versions_ok
echo   checking the working tree too, since it has uncommitted changes ...
echo(
python tools\version.py --check
if errorlevel 1 (
    echo(
    echo ERROR: HEAD agrees with VERSION.md but your working tree does not.
    echo   Sort that out first, so what you test next is what you shipped.
    goto :fail
)
echo(

:versions_ok

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

rem data\defaults-previous is what an update compares a user's display fields
rem against: a field still matching it was ours and can be corrected, anything
rem else is theirs and is left alone. data\default-pages-previous is the same
rem for the MCDU pages. Each has to hold the PREVIOUS release here, not this
rem one, so they are checked before the tag and refreshed after the push.
rem Drift is silent at runtime - no field matches, so no correction reaches
rem anybody - which is why it is caught here instead.
echo   checking the snapshots against the last release ...
python tools\snapshot.py --check
if errorlevel 1 (
    echo(
    echo ERROR: the snapshot is out of step with the last release.
    echo   Fix it, merge it to main through a pull request, then pull and
    echo   re-run.
    goto :fail
)
echo(

rem ==========================================================================
rem  4. release notes
rem ==========================================================================

rem The workflow puts this version's section of CHANGELOG.md at the top of the
rem release notes, and does not fail when there is none: a release nobody wrote
rem an entry for still gets its provenance. Right for the pipeline, wrong for a
rem person, who nearly always meant to write one.
findstr /b /l /c:"## %VERSION%" CHANGELOG.md >nul 2>&1
if not errorlevel 1 (
    echo   CHANGELOG.md has a section for %VERSION%.
    echo(
    goto :notes_ok
)

echo   -----------------------------------------------
echo     Heads up
echo   -----------------------------------------------
echo(
echo   CHANGELOG.md has no '## %VERSION%' section, so the release notes would
echo   carry nothing but the build provenance.
echo(
echo   -----------------------------------------------
set "SURE="
set /p "SURE=Release with no notes? (y/N): "
if /i "!SURE!"=="y"   goto :notes_ok
if /i "!SURE!"=="yes" goto :notes_ok
echo(
echo Cancelled - nothing was tagged or pushed.
goto :end

:notes_ok

rem ==========================================================================
rem  5. message doubles as the final confirmation
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
rem  6. tag and push
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

rem ==========================================================================
rem  7. snapshot these defaults and pages for the next release
rem ==========================================================================

rem Last, and only once the tag is pushed: the release just cut ships the
rem PREVIOUS snapshot, and this sets up the one the NEXT release will compare
rem against. Left unstaged deliberately. The pipeline has only just started, so
rem nothing here is proven yet; if it goes red, throw this away and nothing ever
rem claimed %VERSION% shipped.
echo   snapshotting these defaults and pages for the next release ...
python tools\snapshot.py
if errorlevel 1 (
    echo(
    echo   WARNING: could not refresh data\defaults-previous or
    echo   data\default-pages-previous. The release is fine. Run
    echo   python tools\snapshot.py  before the next one, or updates will stop
    echo   correcting display fields and pages.
    goto :end
)
echo(
echo   Left unstaged. Once the pipeline is green:
echo       git switch -c snapshot-%VERSION%
echo       git add data/defaults-previous data/default-pages-previous
echo       git commit -m "Snapshot %VERSION% defaults and pages"
echo   then open a pull request, so the next release compares against these.
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
