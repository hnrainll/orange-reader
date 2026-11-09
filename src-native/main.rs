mod epub;
mod store;

use epub::Epub;
use gpui::{div, img, prelude::*, px, rgb, App, Application, Bounds, Context, ExternalPaths,
    FocusHandle, IntoElement, KeyDownEvent, PathPromptOptions, Window, WindowBounds, WindowOptions};
use gpui_selectable_text::{register_keyboard_bridge, SelectableText, SelectionChange};
use std::{path::PathBuf, rc::Rc};
use store::{Annotation, Book, Store};

const ORANGE: u32 = 0xd66b3d;

#[derive(Clone)]
struct PagePart { chapter: usize, paragraph: usize, text: String }
struct Page { parts: Vec<PagePart> }

fn paginate(book: &Epub, limit: usize) -> Vec<Page> {
    let mut pages = Vec::new();
    let mut parts = Vec::new();
    let mut length = 0;
    for (chapter, section) in book.chapters.iter().enumerate() {
        for (paragraph, text) in section.paragraphs.iter().enumerate() {
            let chars: Vec<char> = text.chars().collect();
            for chunk in chars.chunks(limit.min(700).max(100)) {
                let chunk = chunk.iter().collect::<String>();
                let count = chunk.chars().count();
                if length + count > limit && !parts.is_empty() {
                    pages.push(Page { parts: std::mem::take(&mut parts) });
                    length = 0;
                }
                parts.push(PagePart { chapter, paragraph, text: chunk });
                length += count + 24;
            }
        }
        if !parts.is_empty() {
            pages.push(Page { parts: std::mem::take(&mut parts) });
            length = 0;
        }
    }
    if !parts.is_empty() { pages.push(Page { parts }); }
    pages
}

struct Library {
    store: Rc<Store>,
    books: Vec<Book>,
    error: Option<String>,
    dark: bool,
    pending_delete: Option<i64>,
}

impl Library {
    fn new(store: Rc<Store>) -> Self {
        let books = store.books().unwrap_or_default();
        let dark = store.setting("theme").as_deref() == Some("dark");
        Self { store, books, error: None, dark, pending_delete: None }
    }

    fn import_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let mut errors = Vec::new();
        for path in paths {
            if !path.extension().and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("epub")) { continue; }
            match epub::parse(&path).and_then(|book| self.store.import(&path, &book)) {
                Ok(_) => {}, Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        self.books = self.store.books().unwrap_or_default();
        self.error = (!errors.is_empty()).then(|| errors.join("\n"));
        cx.notify();
    }

    fn choose_files(&mut self, _: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let result = cx.prompt_for_paths(PathPromptOptions {
            files: true, directories: false, multiple: true, prompt: Some("Import EPUB books".into()),
        });
        cx.spawn(async move |view, cx| {
            if let Ok(Ok(Some(paths))) = result.await {
                let _ = view.update(cx, |library, cx| library.import_paths(paths, cx));
            }
        }).detach();
    }

    fn open_book(&mut self, book: Book, _: &mut Window, cx: &mut Context<Self>) {
        let store = self.store.clone();
        let bounds = Bounds::centered(None, gpui::size(px(1050.), px(780.)), cx);
        let result = cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default()
        }, move |window, cx| {
            let view = cx.new(|cx| Reader::new(store, book, cx));
            window.focus(&view.read(cx).focus_handle);
            view
        });
        if let Err(e) = result { self.error = Some(e.to_string()); cx.notify(); }
    }

    fn delete_book(&mut self, id: i64, _: &mut Window, cx: &mut Context<Self>) {
        if self.pending_delete != Some(id) {
            self.pending_delete = Some(id);
            cx.notify();
            return;
        }
        self.pending_delete = None;
        if let Err(e) = self.store.delete(id) { self.error = Some(e.to_string()); }
        self.books = self.store.books().unwrap_or_default();
        cx.notify();
    }
}

impl Render for Library {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = if self.dark { 0x1c1c1e } else { 0xf8f5f0 };
        let fg = if self.dark { 0xf7f1e9 } else { 0x29211d };
        let card = if self.dark { 0x302b28 } else { 0xffffff };
        div().id("library-root").size_full().flex().flex_col().bg(rgb(bg)).text_color(rgb(fg))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.import_paths(paths.paths().to_vec(), cx);
            }))
            .child(div().h(px(72.)).w_full().flex().items_center().justify_between().px_8()
                .child(div().text_2xl().font_weight(gpui::FontWeight::BOLD).child("Orange Reader"))
                .child(div().flex().gap_3()
                    .child(div().id("theme").cursor_pointer().p_2().child(if self.dark { "☀" } else { "☾" })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.dark = !this.dark;
                            let _ = this.store.save_setting("theme", if this.dark { "dark" } else { "light" });
                            cx.notify();
                        })))
                    .child(div().id("import").cursor_pointer().bg(rgb(ORANGE)).text_color(rgb(0xffffff))
                        .rounded_md().px_4().py_2().child("＋ Import EPUB")
                        .on_click(cx.listener(Self::choose_files)))))
            .when_some(self.error.clone(), |root, error| root.child(div().px_8().py_2()
                .text_color(rgb(0xb43333)).child(error)))
            .child(div().id("library-scroll").flex_1().overflow_y_scroll().p_8()
                .when(self.books.is_empty(), |view| view.child(div().mt_20().text_center()
                    .text_xl().child("Your library is empty. Import an EPUB to start reading.")))
                .child(div().flex().flex_wrap().gap_6().children(self.books.iter().cloned().map(|book| {
                    let open_book = book.clone();
                    let delete_id = book.id;
                    div().id(("book", book.id as usize)).w(px(170.)).p_3().rounded_lg().bg(rgb(card))
                        .shadow_sm().flex().flex_col().gap_2()
                        .child(if let Some(path) = book.cover_path.clone() {
                            div().h(px(210.)).w_full().rounded_md().overflow_hidden()
                                .child(img(path).size_full()).into_any_element()
                        } else {
                            div().h(px(210.)).w_full().rounded_md().bg(rgb(0xe9b48b))
                                .flex().items_center().justify_center().text_size(px(36.))
                                .child(book.title.chars().next().unwrap_or(' ').to_string()).into_any_element()
                        })
                        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(book.title.clone()))
                        .child(div().text_sm().child(book.author.clone()))
                        .child(div().text_xs().text_color(rgb(ORANGE))
                            .child(format!("{}% read", (book.fraction * 100.).round() as u32)))
                        .child(div().flex().justify_between()
                            .child(div().id(("open", book.id as usize)).cursor_pointer().text_color(rgb(ORANGE))
                                .child("Read").on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_book(open_book.clone(), window, cx);
                                })))
                            .child(div().id(("delete", delete_id as usize)).cursor_pointer()
                                .text_color(rgb(0x999999))
                                .child(if self.pending_delete == Some(delete_id) { "Confirm" } else { "Delete" })
                                .on_click(cx.listener(move |this, _, window, cx| this.delete_book(delete_id, window, cx)))))
                }))))
    }
}

struct Reader {
    focus_handle: FocusHandle,
    store: Rc<Store>, book: Book, epub: Option<Epub>, pages: Vec<Page>, page: usize,
    annotations: Vec<Annotation>, selection: Option<(usize, usize, String)>,
    error: Option<String>, show_toc: bool, show_notes: bool, show_settings: bool, dark: bool,
    font_size: f32, line_height: f32, content_width: f32, paragraph_spacing: f32,
    font_family: String,
}

impl Reader {
    fn new(store: Rc<Store>, book: Book, cx: &mut Context<Self>) -> Self {
        let dark = store.setting("theme").as_deref() == Some("dark");
        let font_size = store.setting("font_size").and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(18.).clamp(12., 32.);
        let line_height = store.setting("line_height").and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(1.65).clamp(1.0, 2.4);
        let content_width = store.setting("content_width").and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(780.).clamp(480., 960.);
        let paragraph_spacing = store.setting("paragraph_spacing").and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(1.3).clamp(0.5, 2.5);
        let font_family = store.setting("font_family").unwrap_or_else(|| "System UI".to_owned());
        let mut reader = Self { focus_handle: cx.focus_handle(), store, book, epub: None, pages: Vec::new(), page: 0,
            annotations: Vec::new(), selection: None, error: None,
            show_toc: false, show_notes: false, show_settings: false, dark,
            font_size, line_height, content_width, paragraph_spacing, font_family };
        match epub::parse(&reader.book.file_path) {
            Ok(epub) => {
                if let Err(e) = reader.store.migrate_legacy_annotations(reader.book.id, &epub) {
                    reader.error = Some(format!("Could not migrate highlights: {e}"));
                }
                reader.pages = paginate(&epub, 650);
                if let Ok(Some((chapter, paragraph))) = reader.store.position(reader.book.id) {
                    reader.page = reader.pages.iter().position(|p| p.parts.iter()
                        .any(|part| part.chapter == chapter && part.paragraph == paragraph)).unwrap_or(0);
                } else if reader.book.fraction > 0. && !reader.pages.is_empty() {
                    reader.page = ((reader.pages.len() - 1) as f64 * reader.book.fraction).round() as usize;
                }
                reader.epub = Some(epub);
            }
            Err(e) => reader.error = Some(e.to_string()),
        }
        reader.annotations = reader.store.annotations(reader.book.id).unwrap_or_default();
        reader
    }

    fn move_page(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.pages.is_empty() { return; }
        self.page = self.page.saturating_add_signed(delta).min(self.pages.len() - 1);
        self.selection = None;
        self.save_position();
        cx.notify();
    }

    fn save_position(&mut self) {
        if let Some(part) = self.pages.get(self.page).and_then(|p| p.parts.first()) {
            let fraction = self.page as f64 / self.pages.len().max(1) as f64;
            if let Err(e) = self.store.save_position(self.book.id, part.chapter, part.paragraph, fraction) {
                self.error = Some(e.to_string());
            }
        }
    }

    fn jump_chapter(&mut self, chapter: usize, cx: &mut Context<Self>) {
        if let Some(index) = self.pages.iter().position(|p| p.parts.first().is_some_and(|a| a.chapter == chapter)) {
            self.page = index; self.show_toc = false; self.save_position(); cx.notify();
        }
    }

    fn annotate(&mut self, color: &'static str, cx: &mut Context<Self>) {
        if let Some((chapter, paragraph, text)) = self.selection.take() {
            if let Err(e) = self.store.add_annotation(self.book.id, chapter, paragraph, &text, color) {
                self.error = Some(e.to_string());
            } else {
                self.annotations = self.store.annotations(self.book.id).unwrap_or_default();
            }
            cx.notify();
        }
    }

    fn delete_annotation(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Err(e) = self.store.delete_annotation(id) { self.error = Some(e.to_string()); }
        self.annotations.retain(|a| a.id != id);
        cx.notify();
    }

    fn jump_annotation(&mut self, chapter: usize, paragraph: usize, cx: &mut Context<Self>) {
        if let Some(index) = self.pages.iter().position(|p| p.parts.iter()
            .any(|part| part.chapter == chapter && part.paragraph == paragraph)) {
            self.page = index; self.show_notes = false; self.save_position(); cx.notify();
        }
    }
}

impl Render for Reader {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = if self.dark { 0x1c1c1e } else { 0xfffcf7 };
        let fg = if self.dark { 0xf2ede7 } else { 0x29211d };
        let muted = if self.dark { 0xaaa39e } else { 0x847a73 };
        let chapter = self.pages.get(self.page).and_then(|p| p.parts.first())
            .map(|p| p.chapter).unwrap_or(0);
        let chapter_title = self.epub.as_ref().and_then(|b| b.chapters.get(chapter))
            .map(|c| c.title.as_str()).unwrap_or("").to_owned();
        div().size_full().track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                match event.keystroke.key.as_str() {
                    "left" | "pageup" => this.move_page(-1, cx),
                    "right" | "pagedown" | "space" => this.move_page(1, cx),
                    "escape" => { this.show_toc = false; this.show_notes = false;
                        this.show_settings = false; this.selection = None; cx.notify(); },
                    _ => {},
                }
            }))
            .flex().flex_col().bg(rgb(bg)).text_color(rgb(fg))
            .child(div().h(px(62.)).flex().items_center().justify_between().px_7()
                .child(div().flex().gap_4().items_center()
                    .child(div().id("toc").cursor_pointer().child("☰ Contents")
                        .on_click(cx.listener(|this, _, _, cx| { this.show_toc = !this.show_toc; cx.notify(); })))
                    .child(div().text_sm().text_color(rgb(muted)).child(self.book.title.clone())))
                .child(div().flex().gap_5()
                    .child(div().id("notes").cursor_pointer().child("Highlights")
                        .on_click(cx.listener(|this, _, _, cx| { this.show_notes = !this.show_notes; cx.notify(); })))
                    .child(div().id("settings").cursor_pointer().child("Aa Settings")
                        .on_click(cx.listener(|this, _, _, cx| { this.show_settings = !this.show_settings; cx.notify(); })))))
            .when_some(self.error.clone(), |root, error| root.child(div().px_7().py_2()
                .text_color(rgb(0xb43333)).child(error)))
            .when(self.show_toc, |root| root.child(div().id("toc-scroll").h(px(150.)).overflow_y_scroll().px_7().py_2()
                .bg(rgb(if self.dark { 0x302b28 } else { 0xf2ebe3 }))
                .children(self.epub.as_ref().into_iter().flat_map(|b| b.chapters.iter().enumerate())
                    .map(|(index, item)| div().id(("chapter", index)).cursor_pointer().py_1()
                        .child(item.title.clone()).on_click(cx.listener(move |this, _, _, cx| this.jump_chapter(index, cx)))))))
            .when(self.show_notes, |root| root.child(div().id("notes-scroll").h(px(160.)).overflow_y_scroll().px_7().py_2()
                .bg(rgb(if self.dark { 0x302b28 } else { 0xf2ebe3 }))
                .children(self.annotations.iter().cloned().map(|note| {
                    let id = note.id;
                    let section = note.chapter;
                    let paragraph = note.paragraph;
                    div().flex().items_center().justify_between().py_1().gap_4()
                        .child(div().id(("note", id as usize)).cursor_pointer()
                            .text_color(rgb(u32::from_str_radix(note.color.trim_start_matches('#'), 16).unwrap_or(ORANGE)))
                            .child(format!("{}: {}", section + 1, note.text))
                            .on_click(cx.listener(move |this, _, _, cx| this.jump_annotation(section, paragraph, cx))))
                        .child(div().id(("remove-note", id as usize)).cursor_pointer()
                            .text_color(rgb(muted)).child("Remove")
                            .on_click(cx.listener(move |this, _, _, cx| this.delete_annotation(id, cx))))
                }))))
            .when(self.show_settings, |root| root.child(div().flex().flex_wrap().gap_5().items_center().px_7().py_3()
                .bg(rgb(if self.dark { 0x302b28 } else { 0xf2ebe3 }))
                .child(div().id("smaller").cursor_pointer().child("A−").on_click(cx.listener(|this, _, _, cx| {
                    this.font_size = (this.font_size - 2.).max(12.);
                    let _ = this.store.save_setting("font_size", &this.font_size.to_string()); cx.notify();
                })))
                .child(format!("{} px", self.font_size as u32))
                .child(div().id("larger").cursor_pointer().child("A+").on_click(cx.listener(|this, _, _, cx| {
                    this.font_size = (this.font_size + 2.).min(32.);
                    let _ = this.store.save_setting("font_size", &this.font_size.to_string()); cx.notify();
                })))
                .child(div().id("theme").cursor_pointer().child(if self.dark { "Light" } else { "Dark" })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dark = !this.dark;
                        let _ = this.store.save_setting("theme", if this.dark { "dark" } else { "light" });
                        cx.notify();
                    })))
                .child(div().id("line-smaller").cursor_pointer().child("Line −")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.line_height = (this.line_height - 0.1).max(1.0);
                        let _ = this.store.save_setting("line_height", &this.line_height.to_string()); cx.notify();
                    })))
                .child(format!("Line {:.1}", self.line_height))
                .child(div().id("line-larger").cursor_pointer().child("Line +")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.line_height = (this.line_height + 0.1).min(2.4);
                        let _ = this.store.save_setting("line_height", &this.line_height.to_string()); cx.notify();
                    })))
                .child(div().id("width-smaller").cursor_pointer().child("Width −")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.content_width = (this.content_width - 80.).max(480.);
                        let _ = this.store.save_setting("content_width", &this.content_width.to_string()); cx.notify();
                    })))
                .child(format!("Width {}", self.content_width as u32))
                .child(div().id("width-larger").cursor_pointer().child("Width +")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.content_width = (this.content_width + 80.).min(960.);
                        let _ = this.store.save_setting("content_width", &this.content_width.to_string()); cx.notify();
                    })))
                .child(div().id("font-family").cursor_pointer().child(self.font_family.clone())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.font_family = if this.font_family == "System UI" { "Georgia".to_owned() }
                            else { "System UI".to_owned() };
                        let _ = this.store.save_setting("font_family", &this.font_family); cx.notify();
                    })))
                .child(div().id("paragraph-smaller").cursor_pointer().child("Paragraph −")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.paragraph_spacing = (this.paragraph_spacing - 0.2).max(0.5);
                        let _ = this.store.save_setting("paragraph_spacing", &this.paragraph_spacing.to_string());
                        cx.notify();
                    })))
                .child(div().id("paragraph-larger").cursor_pointer().child("Paragraph +")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.paragraph_spacing = (this.paragraph_spacing + 0.2).min(2.5);
                        let _ = this.store.save_setting("paragraph_spacing", &this.paragraph_spacing.to_string());
                        cx.notify();
                    })))))
            .when_some(self.selection.clone(), |root, _| root.child(div().flex().gap_3().px_7().py_2()
                .child("Highlight selection:")
                .children([("Yellow", "#FFE066"), ("Green", "#86EFAC"), ("Blue", "#93C5FD"),
                    ("Pink", "#F9A8D4"), ("Red", "#FCA5A5")].into_iter().map(|(label, color)|
                    div().id(label).cursor_pointer().text_color(rgb(ORANGE)).child(label)
                        .on_click(cx.listener(move |this, _, _, cx| this.annotate(color, cx)))))))
            .child(div().flex_1().flex().justify_center().overflow_hidden()
                .child(div().id("page-scroll").w(px(self.content_width)).max_w_full().px_10().py_6().overflow_y_scroll()
                    .child(div().text_lg().font_weight(gpui::FontWeight::SEMIBOLD).mb_6()
                        .child(chapter_title.clone()))
                    .children(self.pages.get(self.page).into_iter().flat_map(|p| p.parts.iter())
                        .enumerate().map(|(index, part)| {
                            let section = part.chapter;
                            let paragraph = part.paragraph;
                            let text = part.text.clone();
                            let annotation_color = self.annotations.iter()
                                .find(|a| a.chapter == section && a.paragraph == paragraph)
                                .and_then(|a| u32::from_str_radix(a.color.trim_start_matches('#'), 16).ok());
                            let view = cx.entity().downgrade();
                            div().id(("paragraph", index)).mb(px(self.font_size * self.paragraph_spacing))
                                .text_size(px(self.font_size)).font_family(self.font_family.clone())
                                .line_height(px(self.font_size * self.line_height))
                                .when_some(annotation_color, |d, color| d.bg(rgb(color)).text_color(rgb(0x29211d)))
                                .child(SelectableText::new(("selection", index), text)
                                    .on_selection_change(move |change: &SelectionChange, _, cx| {
                                        let _ = view.update(cx, |reader, cx| {
                                            reader.selection = (!change.text.is_empty())
                                                .then(|| (section, paragraph, change.text.clone()));
                                            cx.notify();
                                        });
                                    }))
                        }))))
            .child(div().h(px(55.)).flex().items_center().justify_between().px_7()
                .child(div().id("previous").cursor_pointer().child("← Previous")
                    .on_click(cx.listener(|this, _, _, cx| this.move_page(-1, cx))))
                .child(div().text_sm().text_color(rgb(muted)).child(format!("{} · {} / {}",
                    chapter_title, self.page + 1, self.pages.len())))
                .child(div().id("next").cursor_pointer().child("Next →")
                    .on_click(cx.listener(|this, _, _, cx| this.move_page(1, cx)))))
    }
}

fn main() {
    let store = match Store::open() { Ok(store) => Rc::new(store), Err(e) => {
        eprintln!("Cannot open library: {e}"); return;
    }};
    Application::new().run(move |cx: &mut App| {
        register_keyboard_bridge(cx).detach();
        let bounds = Bounds::centered(None, gpui::size(px(1100.), px(760.)), cx);
        cx.open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default() }, |_, cx| cx.new(|_| Library::new(store.clone())))
            .expect("Cannot open library window");
        cx.activate(true);
    });
}
