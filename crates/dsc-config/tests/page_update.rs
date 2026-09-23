//! What an update does to the page library and to a profile's page slots.
//!
//! Two checks, each against its own snapshot of what the last release
//! shipped, neither reading the other's files. The rule is the one display
//! fields follow: what is still as we shipped it is ours to correct, and
//! anything the user changed, or deleted, is theirs. See docs/CONFIG.md
//! "Updates: pages and profiles apart".

use std::path::{Path, PathBuf};

use dsc_config::{DeviceInventory, Page, PageFile, PageLibrary, Pages, Profile, Profiles, Readout};

struct Scratch(PathBuf);

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(name: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!(
        "dsc-page-update-{name}-{}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    for sub in ["default-pages", "default-pages-previous", "pages", "defaults", "defaults-previous", "active"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    Scratch(dir)
}

fn field(cells: &str, source: &str) -> Readout {
    let mut f = Readout::reading("", "MCDU", cells.parse().unwrap(), source);
    f.device.clear();
    f
}

fn page(id: &str, name: &str, fields: Vec<Readout>) -> Page {
    Page { id: id.into(), name: name.into(), display: "MCDU".into(), fields }
}

fn write(dir: &Path, sub: &str, pages: Vec<Page>) {
    PageFile { module: "A-10C".into(), pages }.save(&dir.join(sub).join("A-10C.json")).unwrap();
}

fn update(dir: &Path, version: &str) -> Vec<String> {
    Pages::new(dir.join("default-pages"), dir.join("pages")).merge_new(version).expect("the update runs")
}

fn library(dir: &Path) -> PageLibrary {
    PageLibrary::load_dir(&dir.join("pages"))
}

fn cells(p: &Page) -> Vec<(String, String)> {
    p.fields.iter().map(|f| (f.cells.to_string(), f.content[0].source.clone())).collect()
}

#[test]
fn a_page_file_is_seeded_whole_only_where_there_is_none() {
    let dir = scratch("seed");
    write(&dir, "default-pages", vec![page("aaaaaa", "Radios", vec![field("0-1", "A")])]);
    let pages = Pages::new(dir.join("default-pages"), dir.join("pages"));
    assert_eq!(pages.seed().unwrap(), vec!["A-10C.json".to_string()]);
    write(&dir, "pages", Vec::new());
    assert!(pages.seed().unwrap().is_empty(), "a file the user has, emptied or not, is theirs");
    assert!(library(&dir).on_module("A-10C").is_empty());
}

#[test]
fn new_pages_come_in_and_deleted_ones_stay_deleted() {
    let dir = scratch("new-deleted");
    let radios = page("aaaaaa", "Radios", vec![field("0-1", "A")]);
    let fuel = page("bbbbbb", "Fuel", vec![field("24-25", "B")]);
    let nav = page("cccccc", "Nav", vec![field("48-49", "C")]);
    write(&dir, "default-pages-previous", vec![radios.clone(), fuel.clone()]);
    write(&dir, "default-pages", vec![radios.clone(), fuel, nav]);
    // The user deleted Fuel, and made a page of their own called Nav.
    write(&dir, "pages", vec![radios, page("u00001", "nav", vec![field("72-73", "D")])]);

    update(&dir, "2");
    let lib = library(&dir);
    let names: Vec<&str> = lib.on_module("A-10C").iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["Radios", "nav", "Nav 2"], "Fuel stays deleted; Nav comes in numbered");
    assert!(lib.page_on("A-10C", "cccccc").is_some(), "under its shipped id");
}

#[test]
fn a_page_is_corrected_a_field_at_a_time_and_its_name_while_unchanged() {
    let dir = scratch("fields");
    let was = page("aaaaaa", "Radios", vec![field("0-1", "OLD"), field("24-25", "OLD"), field("48-49", "GONE")]);
    let now = page("aaaaaa", "Comms", vec![field("0-1", "NEW"), field("24-25", "NEW"), field("72-73", "ADDED")]);
    let mut mine = was.clone();
    mine.fields[1] = field("24-25", "MINE");
    write(&dir, "default-pages-previous", vec![was]);
    write(&dir, "default-pages", vec![now]);
    write(&dir, "pages", vec![mine]);

    let notes = update(&dir, "2");
    let lib = library(&dir);
    let p = lib.page_on("A-10C", "aaaaaa").unwrap();
    assert_eq!(p.name, "Comms", "the name was still as shipped, so it follows");
    assert_eq!(
        cells(p),
        vec![
            ("0-1".to_string(), "NEW".to_string()),
            ("24-25".to_string(), "MINE".to_string()),
            ("72-73".to_string(), "ADDED".to_string()),
        ]
    );
    assert_eq!(notes.len(), 1, "{notes:?}");

    // Once per version.
    write(&dir, "pages", vec![page("aaaaaa", "Radios", vec![field("0-1", "OLD")])]);
    assert!(update(&dir, "2").is_empty());
    assert_eq!(library(&dir).page_on("A-10C", "aaaaaa").unwrap().name, "Radios");
}

#[test]
fn a_renamed_page_keeps_its_name_and_a_retired_one_goes_only_if_untouched() {
    let dir = scratch("retired");
    let keep = page("aaaaaa", "Radios", vec![field("0-1", "A")]);
    let retire = page("bbbbbb", "Old", vec![field("0-1", "B")]);
    let edited = page("cccccc", "Edited", vec![field("0-1", "C")]);
    write(&dir, "default-pages-previous", vec![keep.clone(), retire.clone(), edited.clone()]);
    write(&dir, "default-pages", vec![page("aaaaaa", "Comms", keep.fields.clone())]);
    let mut mine_keep = keep;
    mine_keep.name = "My radios".into();
    let mut mine_edited = edited;
    mine_edited.fields[0] = field("0-1", "MINE");
    write(&dir, "pages", vec![mine_keep, retire, mine_edited]);

    update(&dir, "2");
    let lib = library(&dir);
    let names: Vec<&str> = lib.on_module("A-10C").iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["My radios", "Edited"]);
}

#[test]
fn a_development_checkout_is_left_alone() {
    let dir = scratch("dev");
    write(&dir, "default-pages", vec![page("aaaaaa", "Radios", vec![field("0-1", "A")])]);
    let pages = Pages::new(dir.join("default-pages"), dir.join("default-pages"));
    assert!(pages.seed().unwrap().is_empty());
    assert!(pages.merge_new("2").unwrap().is_empty());
    assert!(!dir.join("default-pages/.updated").exists());
}

// ------------------------------------------------------------- profile slots

fn inventory() -> DeviceInventory {
    DeviceInventory::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/devices.json")).unwrap()
}

fn profile(start: usize, slots: &[&str]) -> String {
    let slots: Vec<String> = (0..6)
        .map(|i| match slots.get(i).copied() {
            None | Some("") => "null".to_string(),
            Some("blank") => r#"{"page": null, "key": null}"#.to_string(),
            Some(id) => format!(r#"{{"page": "{id}", "key": null}}"#),
        })
        .collect();
    format!(
        r#"{{"schema_version": 2, "name": "Hog", "aircraft": ["A-10C"], "module": "A-10C", "bindings": [],
            "screens": {{"MCDU_Captain": {{"start": {start}, "slots": [{}]}}}}}}"#,
        slots.join(", ")
    )
}

fn lay(dir: &Path, previous: String, shipped: String, mine: String) {
    std::fs::write(dir.join("defaults-previous/a-10c.json"), previous).unwrap();
    std::fs::write(dir.join("defaults/a-10c.json"), shipped).unwrap();
    std::fs::write(dir.join("active/a-10c.json"), mine).unwrap();
}

fn slots_after(dir: &Path) -> (Option<usize>, Vec<Option<Option<String>>>) {
    Profiles::new(dir.join("defaults"), dir.join("active")).merge_new(&inventory(), "2").unwrap();
    let p = Profile::load(&dir.join("active/a-10c.json")).unwrap();
    let s = &p.screens["MCDU_Captain"];
    (s.start, s.slots.iter().map(|s| s.as_ref().map(|s| s.page.clone())).collect())
}

#[test]
fn a_slot_still_as_shipped_takes_the_new_one_and_the_users_stay() {
    let dir = scratch("slots");
    lay(
        &dir,
        profile(1, &["aaaaaa", "bbbbbb", "cccccc", "dddddd"]),
        profile(2, &["aaaaaa", "eeeeee", "ffffff", "gggggg", "blank"]),
        // Slot 3 the user repointed; slot 4 they emptied, as deleting a page does.
        profile(1, &["aaaaaa", "bbbbbb", "uuuuuu", ""]),
    );
    let (start, slots) = slots_after(&dir);
    let page = |id: &str| Some(Some(id.to_string()));
    assert_eq!(slots[0], page("aaaaaa"));
    assert_eq!(slots[1], page("eeeeee"), "unchanged, so the new one");
    assert_eq!(slots[2], page("uuuuuu"), "theirs");
    assert_eq!(slots[3], None, "emptied by the user, and not put back");
    assert_eq!(slots[4], Some(None), "a new blank slot");
    assert_eq!(start, Some(2), "start was as shipped, so it follows");
}

#[test]
fn a_start_the_user_moved_stays() {
    let dir = scratch("start");
    lay(&dir, profile(1, &["aaaaaa", "bbbbbb"]), profile(2, &["aaaaaa", "bbbbbb"]), profile(2, &["aaaaaa", "bbbbbb"]));
    assert_eq!(slots_after(&dir).0, Some(2));
    let dir = scratch("start-theirs");
    lay(&dir, profile(1, &["aaaaaa", "bbbbbb"]), profile(1, &["aaaaaa", "bbbbbb"]), profile(2, &["aaaaaa", "bbbbbb"]));
    assert_eq!(slots_after(&dir).0, Some(2), "theirs, and the release did not move it");
}
