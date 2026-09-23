//! MCDU pages: named screens of text grid fields, kept in a library of their
//! own and pointed at from a profile's slots. See docs/CONFIG.md "MCDU pages".
//!
//! A page is only ever resolved into ordinary display fields, when the engine
//! takes a profile, so nothing that paints or resolves a field learns that
//! pages exist.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{DeviceInventory, DisplayCatalogue, Error, Module, Profile, Readout, Result};

/// How many slots each screen that takes pages has. Always this many, so slot
/// 3 is the same slot whatever is filled around it.
pub const SLOTS: usize = 6;

/// One slot in use: the page it shows, or a blank screen.
///
/// A slot that is not in use is `null` in `slots` rather than one of these.
/// The difference is what a line select key will do once pages can be
/// swapped from the panel: a slot not in use ignores the key and leaves the
/// page shown, and a blank slot takes the screen dark on purpose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    /// The page shown, or none for a blank screen.
    pub page: Option<String>,
    /// Kept for swapping pages from the panel, and null until then. Read as
    /// any value so a profile that sets one is refused by `validate` with a
    /// reason, rather than failing to parse.
    #[serde(default)]
    pub key: Option<serde_json::Value>,
}

impl Slot {
    pub fn new(page: &str) -> Self {
        Slot { page: Some(page.to_string()), key: None }
    }

    /// A slot that shows a blank screen.
    pub fn blank() -> Self {
        Slot { page: None, key: None }
    }
}

/// A screen's six slots and the one shown when a mission starts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageSlots {
    /// The slot shown at mission start, counting from 1. Left out when every
    /// slot is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<usize>,
    #[serde(default)]
    pub slots: Vec<Option<Slot>>,
}

impl Default for PageSlots {
    fn default() -> Self {
        PageSlots { start: None, slots: vec![None; SLOTS] }
    }
}

impl PageSlots {
    /// Every slot in use, blank ones included, counting from 0.
    pub fn filled(&self) -> impl Iterator<Item = (usize, &Slot)> {
        self.slots.iter().enumerate().filter_map(|(i, s)| s.as_ref().map(|s| (i, s)))
    }

    /// Every slot showing a page, with the page's id, counting from 0.
    pub fn pages(&self) -> impl Iterator<Item = (usize, &str)> {
        self.filled().filter_map(|(i, s)| s.page.as_deref().map(|p| (i, p)))
    }

    /// The slot to show at mission start, counting from 0, given which slots
    /// hold a page that will load.
    ///
    /// `start` when its page loads, and otherwise the first slot whose page
    /// does, so a start page that has gone leaves the screen showing the next
    /// one rather than nothing.
    pub fn start_slot(&self, loads: impl Fn(&Slot) -> bool) -> Option<usize> {
        let usable = |i: usize| self.slots.get(i).and_then(Option::as_ref).is_some_and(&loads);
        if let Some(start) = self.start {
            if start >= 1 && usable(start - 1) {
                return Some(start - 1);
            }
        }
        (0..self.slots.len()).find(|&i| usable(i))
    }

    /// Take every slot showing `page` out of use, moving `start` to the first
    /// slot in use if its own was one of them. Returns the slots emptied, from
    /// 1. Out of use rather than blank, so a page deleted never takes a screen
    /// dark that nobody chose to.
    pub fn clear_page(&mut self, page: &str) -> Vec<usize> {
        let mut cleared = Vec::new();
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.as_ref().is_some_and(|s| s.page.as_deref() == Some(page)) {
                *slot = None;
                cleared.push(i + 1);
            }
        }
        self.settle_start();
        cleared
    }

    /// Point `start` at a filled slot: left where it is if that one is filled,
    /// moved to the first filled slot if not, and dropped with nothing filled.
    pub fn settle_start(&mut self) {
        let filled = |i: usize| self.slots.get(i).is_some_and(Option::is_some);
        if self.start.is_some_and(|s| s >= 1 && filled(s - 1)) {
            return;
        }
        self.start = (0..self.slots.len()).find(|&i| filled(i)).map(|i| i + 1);
    }
}

/// A named screen's worth of text grid fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    /// Fixed when the page is made and unique across the whole library. What
    /// a slot points at, so renaming a page rewrites no profile.
    pub id: String,
    /// What the editor shows. Unique within the module, ignoring case and
    /// surrounding space.
    pub name: String,
    /// The display map the fields are drawn on.
    pub display: String,
    /// Display fields without a device, which the slot showing the page
    /// supplies. Held in memory with the page's display filled in, so each is
    /// a field like any other.
    #[serde(default)]
    pub fields: Vec<Readout>,
}

/// Every page on one module, as one file named by the module's catalogue key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageFile {
    pub module: String,
    #[serde(default)]
    pub pages: Vec<Page>,
}

impl PageFile {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let mut file: PageFile =
            serde_json::from_str(&text).map_err(|e| Error::Json(e, path.display().to_string()))?;
        for page in &mut file.pages {
            for f in &mut page.fields {
                f.display = page.display.clone();
                f.device.clear();
            }
        }
        Ok(file)
    }

    /// Write the file in one step, CRLF, as [`Profile::save`] does and for
    /// the same reasons. Each field goes without its device and display, which
    /// are the slot's and the page's.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut bare = self.clone();
        for page in &mut bare.pages {
            for f in &mut page.fields {
                f.display.clear();
                f.device.clear();
            }
        }
        let text = serde_json::to_string_pretty(&bare)
            .map_err(|e| Error::Json(e, path.display().to_string()))?
            .replace('\n', "\r\n");
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temp = path.with_extension("json.saving");
        let written = std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, path));
        if written.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        written?;
        Ok(())
    }
}

/// The file name holding a module's pages.
pub fn page_file_name(module: &str) -> String {
    format!("{module}.json")
}

/// Whether two page names read as the same to a person.
pub fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Every page file in a folder.
#[derive(Debug, Clone, Default)]
pub struct PageLibrary {
    /// By module.
    pub files: BTreeMap<String, PageFile>,
    /// Modules whose file would not load, with why. Every slot on one of these
    /// modules loads empty.
    pub broken: BTreeMap<String, String>,
}

impl PageLibrary {
    /// Load every `*.json` in `dir`. A missing folder is an empty library, and
    /// a file that will not load takes out only its own module's pages.
    pub fn load_dir(dir: &Path) -> Self {
        let mut lib = PageLibrary::default();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return lib;
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();
        paths.sort();
        for path in paths {
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else {
                continue;
            };
            match PageFile::load(&path) {
                Ok(file) if file.module == stem => {
                    lib.files.insert(stem, file);
                }
                // Named by module, so the name is where the daemon and the
                // editor look for it. One saying otherwise would be read as
                // the wrong aircraft's pages by one of them.
                Ok(file) => {
                    lib.broken.insert(
                        stem.clone(),
                        format!("{} says it holds pages for {:?}, not {stem:?}", path.display(), file.module),
                    );
                }
                Err(e) => {
                    lib.broken.insert(stem, e.to_string());
                }
            }
        }
        lib
    }

    /// A library of one module's pages, for tests and previews.
    pub fn of(module: &str, pages: Vec<Page>) -> Self {
        let mut lib = PageLibrary::default();
        lib.files.insert(module.to_string(), PageFile { module: module.to_string(), pages });
        lib
    }

    /// The pages on one module, in file order.
    pub fn on_module(&self, module: &str) -> &[Page] {
        self.files.get(module).map_or(&[][..], |f| &f.pages[..])
    }

    /// A page by id, wherever it is, with the module it is on.
    pub fn find(&self, id: &str) -> Option<(&str, &Page)> {
        self.files
            .iter()
            .find_map(|(m, f)| f.pages.iter().find(|p| p.id == id).map(|p| (m.as_str(), p)))
    }

    /// A page by id, only if it is on `module`.
    pub fn page_on(&self, module: &str, id: &str) -> Option<&Page> {
        self.on_module(module).iter().find(|p| p.id == id)
    }

    /// Why a module's pages did not load, if they did not.
    pub fn broken(&self, module: &str) -> Option<&str> {
        self.broken.get(module).map(String::as_str)
    }

    /// Things wrong with the library as a whole: an id used twice, which would
    /// leave a slot pointing at whichever is found first, and a name used
    /// twice on one module, which the editor could not tell apart.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut ids: BTreeMap<&str, &str> = BTreeMap::new();
        for (module, file) in &self.files {
            let mut names: Vec<&str> = Vec::new();
            for p in &file.pages {
                if let Some(other) = ids.insert(&p.id, module) {
                    out.push(format!(
                        "page id {:?} is used on {other} and again on {module}; slots showing it get the first",
                        p.id
                    ));
                }
                if names.iter().any(|n| same_name(n, &p.name)) {
                    out.push(format!("two pages on {module} are called {:?}", p.name.trim()));
                }
                names.push(&p.name);
            }
        }
        out
    }

    /// An id no page in the library has.
    ///
    /// Six characters of lowercase letters and digits, drawn from the random
    /// seed the standard library keeps for hash maps, which is enough to make
    /// a clash between two users' libraries unlikely and a clash within one
    /// impossible, since it is checked.
    pub fn fresh_id(&self) -> String {
        self.fresh_id_avoiding(&BTreeSet::new())
    }

    /// [`fresh_id`](Self::fresh_id), also avoiding ids not in the library yet:
    /// the ones an import is about to add.
    pub fn fresh_id_avoiding(&self, also: &BTreeSet<String>) -> String {
        const DIGITS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
        loop {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos()),
            );
            let mut n = h.finish();
            let id: String = (0..6)
                .map(|_| {
                    let c = DIGITS[(n % 36) as usize] as char;
                    n /= 36;
                    c
                })
                .collect();
            if self.find(&id).is_none() && !also.contains(&id) {
                return id;
            }
        }
    }

    /// A name on `module` that no page but `except` has: `name` itself, or
    /// `name 2`, `name 3` and so on.
    pub fn free_name(&self, module: &str, name: &str, except: Option<&str>) -> String {
        let taken = |n: &str| {
            self.on_module(module)
                .iter()
                .any(|p| Some(p.id.as_str()) != except && same_name(&p.name, n))
        };
        let base = name.trim();
        if !taken(base) {
            return base.to_string();
        }
        (2..).map(|k| format!("{base} {k}")).find(|n| !taken(n)).expect("some number is free")
    }

    /// Why a page cannot be saved on `module`, if it cannot.
    ///
    /// What does not depend on where it is shown: that it is on a text grid,
    /// that its name is free, and that its fields fit the glass and do not
    /// fight over cells. The font depends on the profile, and is checked for
    /// each profile that shows the page, by [`Profile::problems`].
    pub fn page_problems(
        &self,
        page: &Page,
        module: &Module,
        devices: &DeviceInventory,
        displays: &DisplayCatalogue,
    ) -> Vec<Error> {
        let mut out = Vec::new();
        if page.name.trim().is_empty() {
            out.push(Error::PageUnnamed);
        }
        if self
            .on_module(&module.module)
            .iter()
            .any(|p| p.id != page.id && same_name(&p.name, &page.name))
        {
            out.push(Error::PageNameTaken(page.name.trim().to_string(), module.module.clone()));
        }
        if !displays.get(&page.display).is_some_and(|d| d.is_text_grid()) {
            out.push(Error::PageNotOnTextGrid(page.name.clone(), page.display.clone()));
            return out;
        }
        // Any device carrying the display will do: the checks that remain are
        // about the glass, and a device is only needed to find it.
        let Some(device) = devices.devices.iter().find(|d| d.part_with_display(&page.display).is_some())
        else {
            return out;
        };
        let view = Profile::page_view_on(&module.module, &device.key, page);
        let mut found = Vec::new();
        view.page_field_problems(module, devices, displays, &mut found);
        out.extend(found.into_iter().filter(|e| !e.is_advisory()));
        out
    }

    /// Write one module's pages to `dir`.
    pub fn save_module(&self, dir: &Path, module: &str) -> Result<()> {
        if module.is_empty() || Path::new(module).file_name().and_then(|n| n.to_str()) != Some(module) {
            return Err(Error::NotAProfileFile(module.to_string()));
        }
        let file = self
            .files
            .get(module)
            .cloned()
            .unwrap_or_else(|| PageFile { module: module.to_string(), pages: Vec::new() });
        file.save(&dir.join(page_file_name(module)))
    }
}

/// Shipped pages, and the library in use: the same arrangement as
/// [`Profiles`](crate::Profiles), one file per module in each.
pub struct Pages {
    pub defaults: PathBuf,
    /// The shipped pages as the last release shipped them, or empty for none.
    pub previous: PathBuf,
    pub active: PathBuf,
}

impl Pages {
    pub fn new(defaults: impl Into<PathBuf>, active: impl Into<PathBuf>) -> Self {
        let defaults = defaults.into();
        Pages { previous: crate::snapshot_beside(&defaults), defaults, active: active.into() }
    }

    /// Point somewhere else for the snapshot. Only tests need this.
    pub fn with_previous(mut self, previous: impl Into<PathBuf>) -> Self {
        self.previous = previous.into();
        self
    }

    /// The library in use.
    pub fn library(&self) -> PageLibrary {
        PageLibrary::load_dir(&self.active)
    }

    /// Copy in every shipped page file the library has no file for, returning
    /// the names copied. Creates the library folder if it is missing.
    ///
    /// A whole file at a time, so a first install gets every shipped page. A
    /// module the library already has a file for is reconciled a page at a
    /// time by [`merge_new`](Self::merge_new) instead, which is what keeps a
    /// page the user deleted deleted.
    pub fn seed(&self) -> Result<Vec<String>> {
        if !self.defaults.is_dir() || self.defaults == self.active {
            return Ok(Vec::new());
        }
        std::fs::create_dir_all(&self.active)?;
        let mut shipped: Vec<PathBuf> = std::fs::read_dir(&self.defaults)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();
        shipped.sort();
        let mut copied = Vec::new();
        for from in shipped {
            let Some(name) = from.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            let to = self.active.join(&name);
            if !to.exists() {
                std::fs::copy(&from, &to)?;
                copied.push(name);
            }
        }
        Ok(copied)
    }

    /// Bring the library up to the pages this release ships, keeping every
    /// change the user made. Returns a line for each module it touched.
    ///
    /// Against [`previous`](Self::previous), the pages as the last release
    /// shipped them, and apart from the profiles, which reconcile their slots
    /// against their own snapshot. Neither reads the other's files, and each
    /// leaves the other valid whatever it decides, since a slot only names an
    /// id. See docs/CONFIG.md "Updates: pages and profiles apart".
    ///
    /// * A shipped page the snapshot lacks is new, and comes in, with a number
    ///   added to its name if a page here has it.
    /// * One the snapshot has and the library does not was deleted, and stays
    ///   deleted.
    /// * A page in both is reconciled a field at a time, keyed on cells, by
    ///   the same five cases as a profile's fields. Its name follows the
    ///   shipped one only while it still matches the snapshot's.
    /// * A page the release no longer ships goes only if it is still exactly
    ///   as the snapshot has it.
    ///
    /// Once per version, recorded in the library folder, and never in a
    /// development checkout, where the shipped pages are the library.
    pub fn merge_new(&self, version: &str) -> Result<Vec<String>> {
        if self.defaults == self.active || !self.active.is_dir() {
            return Ok(Vec::new());
        }
        let marker = self.active.join(crate::UPDATED);
        let last = std::fs::read_to_string(&marker).ok().map(|s| s.trim().to_string());
        if last.as_deref() == Some(version) {
            return Ok(Vec::new());
        }
        let shipped = PageLibrary::load_dir(&self.defaults);
        let was = if self.previous.as_os_str().is_empty() {
            PageLibrary::default()
        } else {
            PageLibrary::load_dir(&self.previous)
        };
        let mut lib = self.library();
        let mut notes = Vec::new();

        for (module, file) in &shipped.files {
            // A file of the user's that will not load is left alone, and one
            // they have none of was just seeded whole.
            if lib.broken(module).is_some() || !lib.files.contains_key(module) {
                continue;
            }
            let (mut pages, mut added, mut updated, mut removed, mut renamed, mut retired) = (0, 0, 0, 0, 0, 0);
            for s in &file.pages {
                let w = was.page_on(module, &s.id);
                let at = lib.on_module(module).iter().position(|p| p.id == s.id);
                match (at, w) {
                    (None, None) => {
                        // Shipped ids are for good, but a user's page could
                        // have drawn the same one; theirs stands.
                        if lib.find(&s.id).is_some() {
                            continue;
                        }
                        let name = lib.free_name(module, &s.name, None);
                        let page = Page { name, ..s.clone() };
                        if let Some(f) = lib.files.get_mut(module) {
                            f.pages.push(page);
                        }
                        pages += 1;
                    }
                    (None, Some(_)) => {}
                    (Some(i), w) => {
                        let rename = w.is_some_and(|w| {
                            let mine = &lib.on_module(module)[i];
                            mine.name == w.name && s.name != w.name
                        }) && lib
                            .on_module(module)
                            .iter()
                            .all(|p| p.id == s.id || !same_name(&p.name, &s.name));
                        let Some(mine) = lib.files.get_mut(module).map(|f| &mut f.pages[i]) else {
                            continue;
                        };
                        let before = w.map_or(&[][..], |w| &w.fields[..]);
                        let work = crate::reconcile_field_list(&mut mine.fields, &s.fields, before);
                        updated += work.updated;
                        removed += work.removed;
                        added += work.added;
                        if rename {
                            mine.name = s.name.clone();
                            renamed += 1;
                        }
                    }
                }
            }
            for w in was.on_module(module) {
                if shipped.page_on(module, &w.id).is_some() {
                    continue;
                }
                let untouched = lib.page_on(module, &w.id).is_some_and(|p| {
                    p.name == w.name
                        && p.display == w.display
                        && serde_json::to_value(&p.fields).ok() == serde_json::to_value(&w.fields).ok()
                });
                if untouched {
                    if let Some(f) = lib.files.get_mut(module) {
                        f.pages.retain(|p| p.id != w.id);
                    }
                    retired += 1;
                }
            }
            if pages + added + updated + removed + renamed + retired == 0 {
                continue;
            }
            lib.save_module(&self.active, module)?;
            let mut what = Vec::new();
            for (n, text) in [
                (pages, "new page(s) added"),
                (added, "field(s) added from the shipped pages"),
                (updated, "unchanged field(s) updated to the new pages"),
                (removed, "field(s) removed that the pages no longer ship"),
                (renamed, "unchanged page name(s) updated"),
                (retired, "unchanged page(s) removed that no longer ship"),
            ] {
                if n > 0 {
                    what.push(format!("{n} {text}"));
                }
            }
            notes.push(format!("pages {module}: {}", what.join(", ")));
        }
        std::fs::write(&marker, format!("{version}\r\n"))?;
        Ok(notes)
    }
}

/// A caution about one of a profile's slots, worded to sit on the slot.
#[derive(Debug, Clone, Serialize)]
pub struct SlotNote {
    pub device: String,
    /// Counting from 1, or none for the screen as a whole.
    pub slot: Option<usize>,
    pub text: String,
}

impl Profile {
    /// Whether this profile's pages decide what is on `display`: it is a text
    /// grid, and every field on one comes from a page.
    pub fn takes_pages(displays: &DisplayCatalogue, display: &str) -> bool {
        displays.get(display).is_some_and(|d| d.is_text_grid())
    }

    /// This profile as it runs: each screen's start page put on it as ordinary
    /// display fields, and the slots set aside.
    ///
    /// Done before [`with_followers`](Self::with_followers), so a follower
    /// copies the page along with everything else on the device it follows,
    /// and a follower's own slots are ignored the way its rows are. A slot
    /// whose page is missing, on another module, or in a file that would not
    /// load is empty; see [`slot_notes`](Self::slot_notes) for what to say.
    pub fn with_pages(&self, lib: &PageLibrary) -> Profile {
        let mut p = self.clone();
        p.screens.clear();
        for (device, slots) in &self.screens {
            if self.follows.contains_key(device) {
                continue;
            }
            // A blank slot always loads: it is the screen dark on purpose.
            let loads = |s: &Slot| s.page.as_ref().is_none_or(|id| lib.page_on(&self.module, id).is_some());
            let Some(i) = slots.start_slot(loads) else {
                continue;
            };
            let Some(page) = slots.slots[i]
                .as_ref()
                .and_then(|s| s.page.as_deref())
                .and_then(|id| lib.page_on(&self.module, id))
            else {
                continue;
            };
            p.readouts.extend(page_fields(device, page));
        }
        p
    }

    /// One page shown on one device of this profile, and nothing else of the
    /// profile's rows: what the editor checks a page against, and what the
    /// font check runs on for each slot.
    pub fn page_view(&self, device: &str, page: &Page) -> Profile {
        let mut p = self.clone();
        p.bindings.clear();
        p.screens.clear();
        p.disabled_devices.clear();
        p.follows.clear();
        p.readouts = page_fields(device, page).collect();
        p
    }

    /// A page shown on a device of a profile with no aircraft, so no font:
    /// the checks a page gets before it is shown anywhere.
    fn page_view_on(module: &str, device: &str, page: &Page) -> Profile {
        let mut p = Profile::stub("", "", module, &DeviceInventory { devices: Vec::new() });
        p.aircraft.clear();
        p.readouts = page_fields(device, page).collect();
        p
    }

    /// The field checks, run on a view holding only a page's fields. Cautions
    /// are included; [`Error::is_advisory`] tells them apart.
    pub fn page_field_problems(
        &self,
        module: &Module,
        devices: &DeviceInventory,
        displays: &DisplayCatalogue,
        out: &mut Vec<Error>,
    ) {
        self.readout_problems(module, devices, displays, out, &mut Vec::new());
    }

    /// What is wrong with this profile's slots, as written and as the library
    /// has them: their shape, and each page as it will draw on this profile.
    pub(crate) fn slot_problems(
        &self,
        module: &Module,
        devices: &DeviceInventory,
        displays: &DisplayCatalogue,
        lib: &PageLibrary,
        out: &mut Vec<Error>,
    ) {
        for (device, slots) in &self.screens {
            let Some(spec) = devices.device(device) else {
                out.push(Error::SlotsOnUnknownDevice(device.clone()));
                continue;
            };
            if !spec.displays().any(|(_, d)| Profile::takes_pages(displays, d)) {
                out.push(Error::SlotsWithoutTextGrid(device.clone()));
                continue;
            }
            if slots.slots.len() != SLOTS {
                out.push(Error::SlotCount(device.clone(), slots.slots.len()));
            }
            let any = slots.filled().next().is_some();
            match slots.start {
                Some(s) if s == 0 || s > slots.slots.len() || slots.slots[s - 1].is_none() => {
                    out.push(Error::StartNotFilled(device.clone(), s));
                }
                None if any => out.push(Error::NoStartSlot(device.clone())),
                _ => {}
            }
            for (i, slot) in slots.filled() {
                if slot.key.as_ref().is_some_and(|k| !k.is_null()) {
                    out.push(Error::SlotKeySet(device.clone(), i + 1));
                }
                let Some(id) = slot.page.as_deref() else {
                    continue;
                };
                // Gone from the library is a caution, not a refusal: the slot
                // loads empty and the rest of the profile runs.
                let Some((on, page)) = lib.find(id) else {
                    continue;
                };
                if on != self.module {
                    out.push(Error::PageOnOtherModule(
                        device.clone(),
                        i + 1,
                        page.name.clone(),
                        on.to_string(),
                    ));
                    continue;
                }
                let mut found = Vec::new();
                self.page_view(device, page).page_field_problems(module, devices, displays, &mut found);
                out.extend(found.into_iter().map(|e| {
                    Error::OnPage(page.name.clone(), i + 1, device.clone(), Box::new(e))
                }));
            }
        }
    }

    /// Every slot that loads empty, and why, and where a screen starts on
    /// another slot than it says because of it.
    pub fn slot_notes(&self, lib: &PageLibrary) -> Vec<SlotNote> {
        let mut out = Vec::new();
        for (device, slots) in &self.screens {
            if self.follows.contains_key(device) {
                continue;
            }
            if let Some(why) = lib.broken(&self.module) {
                if slots.filled().next().is_some() {
                    out.push(SlotNote {
                        device: device.clone(),
                        slot: None,
                        text: format!(
                            "The pages for {} did not load, so every slot is empty: {why}",
                            self.module
                        ),
                    });
                }
                continue;
            }
            let loads = |s: &Slot| s.page.as_ref().is_none_or(|id| lib.page_on(&self.module, id).is_some());
            for (i, id) in slots.pages() {
                if lib.find(id).is_none() {
                    out.push(SlotNote {
                        device: device.clone(),
                        slot: Some(i + 1),
                        text: format!(
                            "Slot {} shows page {id:?}, which is not in the library, so it is empty.",
                            i + 1,
                        ),
                    });
                }
            }
            let starts = slots.start_slot(loads).map(|i| i + 1);
            if let (Some(said), Some(real)) = (slots.start, starts) {
                if said != real {
                    out.push(SlotNote {
                        device: device.clone(),
                        slot: Some(said),
                        text: format!("The screen starts on slot {real} instead, since slot {said} is empty."),
                    });
                }
            }
        }
        out
    }

    /// Every page id this profile's slots show.
    pub fn pages_used(&self) -> BTreeSet<String> {
        self.screens.values().flat_map(|s| s.pages().map(|(_, id)| id.to_string())).collect()
    }
}

/// A page's fields as fields of `device`, marked as the page's.
fn page_fields<'a>(device: &'a str, page: &'a Page) -> impl Iterator<Item = Readout> + 'a {
    page.fields.iter().map(move |f| {
        let mut f = f.clone();
        f.device = device.to_string();
        f.display = page.display.clone();
        f.page = Some(page.id.clone());
        f
    })
}
