//! Pages as the editor works with them: one module's pages at a time.
//!
//! A page belongs to the library, not to the profile open in the window, so
//! it is saved and deleted on its own and the profile's Save writes only which
//! page sits in which slot. Every profile on the module shows the same page,
//! so editing one here edits it everywhere: the window is told where each page
//! is used so it can say so, and deleting one empties the slots showing it in
//! every profile.

use std::collections::BTreeSet;

use dsc_config::paths::Paths;
use dsc_config::{Page, PageFile, PageLibrary, Profile};

use crate::{fail, Reply};

/// One slot showing a page, somewhere in the active profiles.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PageUse {
    pub page: String,
    pub file: String,
    pub profile: String,
    pub device: String,
    /// Counting from 1.
    pub slot: usize,
}

/// A module's pages, and everything the window needs to show them.
#[derive(Debug, serde::Serialize)]
pub struct PagesView {
    pub pages: Vec<Page>,
    /// Why the module's page file would not load, when it would not. Its
    /// pages cannot be edited until it is fixed by hand, since saving would
    /// write over what is there.
    pub broken: Option<String>,
    pub used: Vec<PageUse>,
    /// The module's pages as they shipped, so one field of a shipped page can
    /// be put back without touching the rest. Empty for a module that ships
    /// none; a page the user made is simply not in it.
    pub shipped: Vec<Page>,
}

/// Pages as the window sends them, put the way a page file loads: every
/// field on its page's display and on no device. A field the window added is
/// made on the device whose screen it was added to, which the page does not
/// keep.
pub fn tidy(pages: &[Page]) -> Vec<Page> {
    let mut pages = pages.to_vec();
    for page in &mut pages {
        for f in &mut page.fields {
            f.device.clear();
            f.display = page.display.clone();
            f.page = None;
        }
    }
    pages
}

/// The library in use, with the page being edited in place of the saved
/// one with its id, or added when it is new.
pub fn library_with(paths: &Paths, module: &str, working: &Page) -> PageLibrary {
    let mut lib = paths.pages.library();
    let working = tidy(std::slice::from_ref(working)).remove(0);
    let file = lib
        .files
        .entry(module.to_string())
        .or_insert_with(|| PageFile { module: module.to_string(), pages: Vec::new() });
    match file.pages.iter_mut().find(|p| p.id == working.id) {
        Some(there) => *there = working,
        None => file.pages.push(working),
    }
    lib
}

/// Every active profile on `module`, by file name.
fn profiles_on(paths: &Paths, module: &str) -> Vec<(String, Profile)> {
    let Ok(entries) = std::fs::read_dir(&paths.profiles.active) else {
        return Vec::new();
    };
    let mut out: Vec<(String, Profile)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .filter_map(|p| {
            let file = p.file_name()?.to_string_lossy().into_owned();
            let profile = Profile::load(&p).ok()?;
            (profile.module == module).then_some((file, profile))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Every slot on `module` showing a page, across the active profiles.
pub fn usage(paths: &Paths, module: &str) -> Vec<PageUse> {
    let mut out = Vec::new();
    for (file, p) in profiles_on(paths, module) {
        for (device, slots) in &p.screens {
            // A follower's slots are kept and not shown, so they use nothing.
            if p.follows.contains_key(device) {
                continue;
            }
            for (i, id) in slots.pages() {
                out.push(PageUse {
                    page: id.to_string(),
                    file: file.clone(),
                    profile: p.name.clone(),
                    device: device.clone(),
                    slot: i + 1,
                });
            }
        }
    }
    out
}

/// One module's pages, for a profile on it being opened.
#[tauri::command]
pub fn open_pages(module: String) -> Reply<PagesView> {
    let paths = Paths::resolve();
    let lib = paths.pages.library();
    Ok(PagesView {
        pages: lib.on_module(&module).to_vec(),
        broken: lib.broken(&module).map(str::to_string),
        used: usage(&paths, &module),
        shipped: PageLibrary::load_dir(&paths.pages.defaults).on_module(&module).to_vec(),
    })
}

/// An id for a new page: none in the library has it, and neither do `avoid`,
/// the new pages the window holds that are not saved yet.
#[tauri::command]
pub fn new_page_id(avoid: Vec<String>) -> Reply<String> {
    let paths = Paths::resolve();
    let avoid: BTreeSet<String> = avoid.into_iter().collect();
    Ok(paths.pages.library().fresh_id_avoiding(&avoid))
}

/// Save one page into its module's file, adding it or replacing the page with
/// its id, and hand back the module's pages as they now are.
///
/// Refused when the page could not be shown: a name another page on the
/// module has, glass that is not a text grid, fields that overlap or run off
/// the screen, or characters the font of the profile being edited cannot
/// draw. A page is saved on its own, apart from the profile, because it is
/// the library's rather than the profile's: every profile on the module shows
/// the same page.
#[tauri::command]
pub fn save_page(
    profile: Profile,
    page: Page,
    device: String,
    cache: tauri::State<crate::check::Cache>,
) -> Reply<PagesView> {
    let paths = Paths::resolve();
    let module = profile.module.clone();
    let mut lib = paths.pages.library();
    if let Some(why) = lib.broken(&module) {
        return Err(format!(
            "{} was not saved, because the page file for {module} would not load and saving would replace it: {why}",
            page.name.trim()
        ));
    }
    let mut page = tidy(std::slice::from_ref(&page)).remove(0);
    page.name = page.name.trim().to_string();
    let problems = cache.page_problems(&paths, &lib, &profile, &page, &device);
    if !problems.is_empty() {
        return Err(format!("{} was not saved:\n{}", page.name, problems.join("\n")));
    }

    let file = lib
        .files
        .entry(module.clone())
        .or_insert_with(|| PageFile { module: module.clone(), pages: Vec::new() });
    match file.pages.iter_mut().find(|p| p.id == page.id) {
        Some(there) => *there = page,
        None => file.pages.push(page),
    }
    lib.save_module(&paths.pages.active, &module).map_err(|e| fail(&format!("writing the pages for {module}"), e))?;
    open_pages(module)
}

/// Delete a page from its module's file, emptying every slot showing it in
/// the saved profiles on the module but `current`, whose slots the window
/// empties itself so that its own Save stays the one that writes it.
///
/// Returns the module's pages as they now are, and names the profiles changed.
#[tauri::command]
pub fn delete_page(module: String, id: String, current: String) -> Reply<(PagesView, Vec<String>)> {
    let paths = Paths::resolve();
    let mut lib = paths.pages.library();
    if let Some(why) = lib.broken(&module) {
        return Err(format!("the page file for {module} would not load, so nothing was deleted: {why}"));
    }
    if let Some(file) = lib.files.get_mut(&module) {
        file.pages.retain(|p| p.id != id);
    }
    lib.save_module(&paths.pages.active, &module).map_err(|e| fail(&format!("writing the pages for {module}"), e))?;

    let mut touched = Vec::new();
    for (name, mut p) in profiles_on(&paths, &module) {
        if name == current {
            continue;
        }
        let changed = p.screens.values_mut().fold(false, |any, s| !s.clear_page(&id).is_empty() || any);
        if changed {
            p.save(&paths.profiles.active.join(&name)).map_err(|e| fail(&format!("writing {name}"), e))?;
            touched.push(p.name);
        }
    }
    Ok((open_pages(module)?, touched))
}
