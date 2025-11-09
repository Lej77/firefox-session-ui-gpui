#![cfg_attr(
    not(feature = "real_data"),
    allow(dead_code, unused_variables, unused_imports)
)]

use std::{
    borrow::Cow, env, future::Future, io::Empty, path::PathBuf, sync::Arc, time::SystemTime,
};

use either::Either;
#[cfg(feature = "real_data")]
use firefox_session_data::session_store::FirefoxSessionStore;
#[cfg(feature = "real_data")]
pub use firefox_session_data::to_links::ttl_formats::FormatInfo;

/// Unconditionally sendable when targeting the web.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub struct WebSendable<T>(pub T);

// Safety: only a single thread when targeting the web.
//         https://doc.rust-lang.org/nightly/rustc/platform-support/wasm32-unknown-unknown.html#conditionally-compiling-code
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
unsafe impl<T> Send for WebSendable<T> {}
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
unsafe impl<T> Sync for WebSendable<T> {}

/// A version of [`tokio::task::spawn_blocking`] that works for the WebAssembly
/// target where we don't have access to threads, in that case we simply block
/// the runtime (i.e. the event loop).
#[cfg(feature = "real_data")]
pub async fn spawn_blocking<F, R>(f: F) -> R
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    #[cfg(target_family = "wasm")]
    {
        f()
    }
    #[cfg(not(target_family = "wasm"))]
    {
        tokio::task::spawn_blocking(f).await.unwrap()
    }
}

#[cfg(not(feature = "real_data"))]
mod fake {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FormatInfo {
        PDF,
        Text,
        Html,
    }
    impl FormatInfo {
        pub fn all() -> &'static [Self] {
            &[Self::PDF, Self::Text, Self::Html]
        }
        pub fn as_str(&self) -> &'static str {
            match self {
                FormatInfo::PDF => "pdf",
                FormatInfo::Text => "text",
                FormatInfo::Html => "html",
            }
        }
    }
    impl std::fmt::Display for FormatInfo {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                FormatInfo::PDF => write!(f, "Convert the links to a PDF that contains links."),
                FormatInfo::Text => write!(f, "Convert the links to raw text."),
                FormatInfo::Html => write!(f, "Convert the links to a HTML file with anchor tags."),
            }
        }
    }

    pub(super) type FirefoxSessionStore = ();

    impl FileInfo {
        pub async fn load_data(&mut self) -> Result<(), String> {
            self.data = Some(FileData::Compressed(Arc::new([])));
            Ok(())
        }

        pub async fn decompress_data(&mut self) -> Result<(), String> {
            self.data = Some(FileData::Uncompressed(Arc::new([])));
            Ok(())
        }

        pub async fn parse_session_data(&mut self) -> Result<(), String> {
            self.data = Some(FileData::Parsed(Arc::new(())));
            Ok(())
        }

        pub async fn get_groups_from_session(
            &self,
            sort_groups: bool,
        ) -> Result<AllTabGroups, String> {
            Ok(AllTabGroups {
                open: vec![
                    TabGroup {
                        index: 0,
                        name: "Window 1".into(),
                    },
                    TabGroup {
                        index: 1,
                        name: "Window 2".into(),
                    },
                ],
                closed: vec![TabGroup {
                    index: 2,
                    name: "Closed window 1".into(),
                }],
            })
        }

        pub async fn to_text_links(
            &self,
            generate_options: GenerateOptions,
        ) -> Result<String, String> {
            Ok("http://www.example.com".to_string())
        }

        pub async fn save_links(
            &self,
            save_path: PathBuf,
            generate_options: GenerateOptions,
            output_options: OutputOptions,
        ) -> Result<(), String> {
            Ok(())
        }
    }
}
#[cfg(not(feature = "real_data"))]
pub use fake::*;

#[derive(Debug, Clone)]
pub struct FirefoxProfileInfo {
    path: PathBuf,
    #[expect(dead_code, reason = "we don't expose this field currently")]
    modified_at: Result<SystemTime, String>,
}
impl FirefoxProfileInfo {
    /// Name of the Firefox profile folder.
    pub fn name(&self) -> Cow<'_, str> {
        self.path.file_name().unwrap_or_default().to_string_lossy()
    }
    pub fn find_sessionstore_file(&self) -> PathBuf {
        let previous = self.path.join("sessionstore-backups/previous.jsonlz4");
        let recovery_older = self.path.join("sessionstore-backups/recovery.baklz4");
        let recovery = self.path.join("sessionstore-backups/recovery.jsonlz4");
        let session_store = self.path.join("sessionstore.jsonlz4");

        if session_store.exists() {
            // If Firefox is closed then this has the latest data:
            session_store
        } else if recovery.exists() {
            // Otherwise this keeps the latest data:
            recovery
        } else if recovery_older.exists() {
            // When Firefox is overwriting recovery it moves
            // the old one here first:
            recovery_older
        } else if previous.exists() {
            // And this is from the previous startup:
            previous
        } else {
            // This is written most often so keep trying it:
            recovery
        }
    }
    pub fn all_profiles() -> Vec<FirefoxProfileInfo> {
        #[cfg(feature = "real_data")]
        let profiles = ::firefox_session_data::find::FirefoxProfileFinder::new()
            .and_then(|finder| {
                Ok(finder
                    .all_profiles()?
                    .iter()
                    .map(|(p, t)| (p.clone(), t.as_ref().map_err(|e| e.to_string()).copied()))
                    .map(|(path, modified_at)| FirefoxProfileInfo { path, modified_at })
                    .collect::<Vec<_>>())
            })
            .unwrap_or_default();

        #[cfg(not(feature = "real_data"))]
        let profiles: Vec<FirefoxProfileInfo> = vec![FirefoxProfileInfo {
            path: "./firefox-profiles/02921.default-release".into(),
            modified_at: Err("Not available".to_string()),
        }];

        profiles
    }
}

/// An object safe trait that can be used by
/// [`rfd::AsyncFileDialog::set_parent`].
pub trait DialogParent:
    raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle
{
}
impl<W> DialogParent for W where
    W: raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle
{
}

/// Wrap a type that provides a [`raw_window_handle::WindowHandle`] but doesn't
/// provide a [`raw_window_handle::DisplayHandle`] and makes it usable with
/// [`prompt_load_file`] and [`prompt_save_file`].
pub struct NoDisplayHandle<W>(pub W);
impl<W> raw_window_handle::HasWindowHandle for NoDisplayHandle<W>
where
    W: raw_window_handle::HasWindowHandle,
{
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        raw_window_handle::HasWindowHandle::window_handle(&self.0)
    }
}
impl<W> raw_window_handle::HasDisplayHandle for NoDisplayHandle<W>
where
    W: raw_window_handle::HasDisplayHandle,
{
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        // gpui panics if its `<gpui::Window as
        // HasDisplayHandle>::display_handle` method is called on Windows
        Err(raw_window_handle::HandleError::NotSupported)
    }
}

/// Also consider [`gpui::App::prompt_for_paths`].
pub fn prompt_load_file(
    parent: Option<&dyn DialogParent>,
) -> impl Future<Output = Option<rfd::FileHandle>> + 'static {
    let mut builder = ::rfd::AsyncFileDialog::new()
        .add_filter("Firefox session file", &["js", "baklz4", "jsonlz4"])
        .add_filter("All files", &["*"])
        .set_title("Open Firefox Sessionstore File");

    if let Some(parent) = parent {
        builder = builder.set_parent(&parent);
    }

    if let Some(data) = env::var_os("APPDATA") {
        let data = PathBuf::from(data);
        builder = builder.set_directory(data.join("Mozilla\\Firefox\\Profiles"));
    }

    builder.pick_file()
}

/// Also consider [`gpui::App::prompt_for_new_path`].
pub fn prompt_save_file(
    parent: Option<&dyn DialogParent>,
) -> impl Future<Output = Option<rfd::FileHandle>> + 'static {
    let mut builder = rfd::AsyncFileDialog::new()
        // Don't specify filters, otherwise we must end filenames with a dot!
        //.add_filter("All files", &["*"])
        //.add_filter("Text files", &["txt"])
        //.add_filter("Rich text format files", &["rtf"])
        //.add_filter("HTML files", &["html"])
        //.add_filter("Markdown files", &["md"])
        //.add_filter("Typst files", &["typ"])
        //.add_filter("PDF files", &["pdf"])
        .set_title("Save Links from Firefox Tabs");

    if let Some(parent) = parent {
        builder = builder.set_parent(&parent);
    }

    builder.save_file()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabGroup {
    /// The index of the tab group. Used to uniquely identify a group in a
    /// specific session.
    pub index: u32,
    /// Name of a tab group.
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AllTabGroups {
    /// Tab groups or windows.
    pub open: Vec<TabGroup>,
    /// Tab groups or windows in recently closed windows.
    pub closed: Vec<TabGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateOptions {
    pub open_group_indexes: Option<Vec<u32>>,
    pub closed_group_indexes: Option<Vec<u32>>,
    pub sort_groups: bool,
    pub table_of_content: bool,
}
impl GenerateOptions {
    pub fn selected_groups(&self) -> usize {
        self.open_group_indexes.as_ref().map_or(0, Vec::len)
            + self.closed_group_indexes.as_ref().map_or(0, Vec::len)
    }
}
impl Default for GenerateOptions {
    fn default() -> Self {
        Self {
            open_group_indexes: None,
            closed_group_indexes: Some(Vec::new()),
            sort_groups: true,
            table_of_content: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputOptions {
    pub format: FormatInfo,
    pub overwrite: bool,
    pub create_folder: bool,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            format: FormatInfo::PDF,
            overwrite: Default::default(),
            create_folder: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum FileData {
    Compressed(Arc<[u8]>),
    Uncompressed(Arc<[u8]>),
    Parsed(Arc<FirefoxSessionStore>),
}
impl FileData {
    pub fn as_parsed(&self) -> Option<&Arc<FirefoxSessionStore>> {
        if let Self::Parsed(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub file_path: Arc<PathBuf>,
    pub file_handle: Option<rfd::FileHandle>,
    pub data: Option<FileData>,
}
impl FileInfo {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path: Arc::new(file_path),
            file_handle: None,
            data: None,
        }
    }
    pub fn is_compressed_file_format(&self) -> bool {
        self.file_path
            .extension()
            .and_then(|ext| ext.to_str().map(|v| v.ends_with("lz4")))
            .unwrap_or(false)
    }
}
#[cfg(feature = "real_data")]
impl FileInfo {
    pub async fn load_data(&mut self) -> Result<(), String> {
        if self.data.is_some() {
            return Ok(());
        }

        #[cfg(target_family = "wasm")]
        let data = self
            .file_handle
            .as_ref()
            .ok_or("no file handle for the specified path")?
            .0
            .read()
            .await;

        #[cfg(not(target_family = "wasm"))]
        let data = {
            use std::{
                fs::File,
                io::{BufReader, Read},
            };

            let path = self.file_path.clone();
            spawn_blocking(move || -> Result<_, String> {
                let file = File::open(&*path)
                    .map_err(|e| format!("failed to open file at {}: {e}", path.display()))?;

                let mut buffer = BufReader::new(file);
                let mut data = Vec::new();

                buffer.read_to_end(&mut data).map_err(|e| {
                    format!("failed to read file data from {}: {e}", path.display())
                })?;

                Ok(data)
            })
            .await?
        };

        let data = Arc::from(data);
        self.data = Some(if self.is_compressed_file_format() {
            FileData::Compressed(data)
        } else {
            FileData::Uncompressed(data)
        });

        Ok(())
    }
    pub async fn decompress_data(&mut self) -> Result<(), String> {
        let data = match self
            .data
            .as_ref()
            .ok_or("Tried to decompress data before reading it")?
        {
            FileData::Compressed(data) => data.clone(),
            FileData::Uncompressed(_) | FileData::Parsed(_) => return Ok(()),
        };
        let decompressed = spawn_blocking(move || {
            firefox_session_data::io_utils::decompress_lz4_data(Either::<_, Empty>::Left(
                Vec::<u8>::from(&*data).into(),
            ))
            .map(|reader| -> Vec<u8> { reader.into() })
            .map_err(|e| format!("failed to decompress data: {e}"))
        })
        .await?;

        self.data = Some(FileData::Uncompressed(Arc::from(decompressed)));
        Ok(())
    }
    pub async fn parse_session_data(&mut self) -> Result<(), String> {
        let data = match self
            .data
            .as_ref()
            .ok_or("Tried to parse data before reading it")?
        {
            FileData::Compressed(_) => return Err("can't parse compressed data".to_string()),
            FileData::Uncompressed(data) => data.clone(),
            FileData::Parsed(_) => return Ok(()),
        };
        let session = spawn_blocking(move || {
            serde_json::from_slice::<FirefoxSessionStore>(&data)
                .map_err(|e| format!("failed to parse sessionstore JSON data: {e}"))
        })
        .await?;

        self.data = Some(FileData::Parsed(Arc::new(session)));
        Ok(())
    }
    pub async fn get_groups_from_session(&self, sort_groups: bool) -> Result<AllTabGroups, String> {
        use firefox_session_data::session_store::session_info::get_groups_from_session;

        let session = self
            .data
            .as_ref()
            .and_then(FileData::as_parsed)
            .cloned()
            .ok_or("must deserialize JSON sessionstore data before tab groups can be inspected")?;

        Ok(spawn_blocking(move || AllTabGroups {
            open: get_groups_from_session(&session, true, false, sort_groups)
                .enumerate()
                .map(|(ix, group)| TabGroup {
                    index: ix as _,
                    name: group.name().to_owned(),
                })
                .collect::<Vec<_>>(),
            closed: get_groups_from_session(&session, false, true, sort_groups)
                .enumerate()
                .map(|(ix, group)| TabGroup {
                    index: ix as _,
                    name: group.name().to_owned(),
                })
                .collect::<Vec<_>>(),
        })
        .await)
    }

    /// Generate a text only representation of the sessionstore data.
    pub async fn to_text_links(&self, generate_options: GenerateOptions) -> Result<String, String> {
        use firefox_session_data::{
            pdf_converter::html_to_pdf::WriteBuilderSimple,
            session_store::{
                session_info::{get_groups_from_session, TreeDataSource},
                to_links::LinkFormat,
                to_links::ToLinksOptions,
            },
            to_links::TabsToLinksOutput,
        };

        let session = self
            .data
            .as_ref()
            .and_then(FileData::as_parsed)
            .cloned()
            .ok_or("must deserialize JSON sessionstore data before converting tabs to links")?;

        spawn_blocking(move || {
            let mut output: Vec<u8> = Vec::new();

            let open_groups =
                get_groups_from_session(&session, true, false, generate_options.sort_groups)
                    .enumerate()
                    .filter(|(ix, _)| {
                        if let Some(indexes) = &generate_options.open_group_indexes {
                            indexes.contains(&(*ix as u32))
                        } else {
                            true
                        }
                    })
                    .map(|(_, g)| g);

            let closed_groups =
                get_groups_from_session(&session, false, true, generate_options.sort_groups)
                    .enumerate()
                    .filter(|(ix, _)| {
                        if let Some(indexes) = &generate_options.closed_group_indexes {
                            indexes.contains(&(*ix as u32))
                        } else {
                            true
                        }
                    })
                    .map(|(_, g)| g);

            firefox_session_data::tabs_to_links(
                &open_groups.chain(closed_groups).collect::<Vec<_>>(),
                TabsToLinksOutput {
                    format: LinkFormat::TXT,
                    as_pdf: None,
                    conversion_options: ToLinksOptions {
                        format: LinkFormat::TXT,
                        page_breaks_after_group: false, // We don't have any page break character in raw text
                        skip_page_break_after_last_group: true,
                        table_of_contents: generate_options.table_of_content,
                        indent_all_links: true,
                        custom_page_break: "".into(),
                        // If there is any data from Sidebery then TST data
                        // won't be used and so on:
                        tree_sources: (&[
                            TreeDataSource::Sidebery,
                            TreeDataSource::TstWebExtension,
                            TreeDataSource::TstLegacy,
                        ] as &[_])
                            .into(),
                    },
                },
                WriteBuilderSimple(&mut output),
            )
            .map_err(|e| e.to_string())?;

            Ok(String::from_utf8_lossy(&output).into_owned())
        })
        .await
    }
    #[cfg_attr(target_family = "wasm", expect(unused_mut, unused_variables))]
    pub async fn save_links(
        &self,
        mut save_path: PathBuf,
        generate_options: GenerateOptions,
        output_options: OutputOptions,
    ) -> Result<(), String> {
        use firefox_session_data::{
            pdf_converter::html_to_pdf::WriteBuilderSimple,
            session_store::{
                session_info::{get_groups_from_session, TreeDataSource},
                to_links::LinkFormat,
                to_links::ToLinksOptions,
            },
            to_links::TabsToLinksOutput,
        };

        let session = self
            .data
            .as_ref()
            .and_then(FileData::as_parsed)
            .cloned()
            .ok_or("must deserialize JSON sessionstore data before converting tabs to links")?;

        spawn_blocking(move || {
            let (format, as_pdf) = output_options.format.as_format().to_link_format();

            let file_ext = if as_pdf.is_some() {
                "pdf"
            } else {
                match format {
                    LinkFormat::TXT => "txt",
                    LinkFormat::RTF { .. } => "rtf",
                    LinkFormat::HTML => "html",
                    LinkFormat::Markdown => "md",
                    LinkFormat::Typst => "typ",
                }
            };

            let mut file = {
                #[cfg(target_family = "wasm")]
                {
                    Vec::new()
                }
                #[cfg(not(target_family = "wasm"))]
                {
                    if save_path.extension().is_none() {
                        save_path.set_extension(file_ext);
                    }

                    if let Some(folder) = save_path.parent() {
                        if output_options.create_folder {
                            std::fs::create_dir_all(folder).map_err(|e| {
                                format!("failed to create folder at \"{}\": {e}", folder.display())
                            })?;
                        }
                    }

                    std::fs::OpenOptions::new()
                        .write(true)
                        .truncate(true)
                        .create(true)
                        .create_new(!output_options.overwrite)
                        .open(&save_path)
                        .map_err(|e| {
                            format!(
                                "failed to create new file at \"{}\": {e}",
                                save_path.display()
                            )
                        })?
                }
            };

            let open_groups =
                get_groups_from_session(&session, true, false, generate_options.sort_groups)
                    .enumerate()
                    .filter(|(ix, _)| {
                        if let Some(indexes) = &generate_options.open_group_indexes {
                            indexes.contains(&(*ix as u32))
                        } else {
                            true
                        }
                    })
                    .map(|(_, g)| g);

            let closed_groups =
                get_groups_from_session(&session, false, true, generate_options.sort_groups)
                    .enumerate()
                    .filter(|(ix, _)| {
                        if let Some(indexes) = &generate_options.closed_group_indexes {
                            indexes.contains(&(*ix as u32))
                        } else {
                            true
                        }
                    })
                    .map(|(_, g)| g);

            let page_breaks = !matches!(output_options.format, FormatInfo::TEXT);
            firefox_session_data::tabs_to_links(
                &open_groups.chain(closed_groups).collect::<Vec<_>>(),
                TabsToLinksOutput {
                    format,
                    as_pdf,
                    conversion_options: ToLinksOptions {
                        format,
                        page_breaks_after_group: page_breaks,
                        skip_page_break_after_last_group: page_breaks
                            && (format.is_html() || format.is_typst()),
                        table_of_contents: generate_options.table_of_content,
                        indent_all_links: true,
                        custom_page_break: "".into(),
                        // First found data is used so if there is any data from
                        // Sidebery then TST data won't be used at all:
                        tree_sources: (&[
                            TreeDataSource::Sidebery,
                            TreeDataSource::TstWebExtension,
                            TreeDataSource::TstLegacy,
                        ] as &[_])
                            .into(),
                    },
                },
                WriteBuilderSimple(&mut file),
            )
            .map_err(|e| e.to_string())?;

            #[cfg(target_family = "wasm")]
            save_file_on_web_target(file.as_slice(), Some(&format!("firefox-links.{file_ext}")))?;

            Ok(())
        })
        .await
    }
}

/// Save some data to a file and download it via the user's browser.
///
/// # References
///
/// <https://stackoverflow.com/questions/54626186/how-to-download-file-with-javascript>
/// <https://stackoverflow.com/questions/44147912/arraybuffer-to-blob-conversion>
#[cfg(target_family = "wasm")]
fn save_file_on_web_target(data: &[u8], file_name: Option<&str>) -> Result<(), String> {
    use wasm_bindgen::JsCast;

    let byte_array = js_sys::Uint8Array::new_with_length(
        data.len()
            .try_into()
            .map_err(|e| format!("Output file size was larger than a 32 bit number {e}"))?,
    );
    byte_array.copy_from(data);
    let array = js_sys::Array::of1(&byte_array);
    let blob = web_sys::Blob::new_with_u8_array_sequence(&array)
        .ok()
        .ok_or("Blob creation failed")?;

    let a_tag: web_sys::HtmlAnchorElement = web_sys::window()
        .ok_or("no global window")?
        .document()
        .ok_or("no \"window.document\"")?
        .create_element("a")
        .map_err(|_| "failed to create \"a\" tag")?
        .unchecked_into();

    if let Some(file_name) = file_name {
        a_tag.set_download(file_name);
    }

    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .ok()
        .ok_or("url creation failed")?;

    a_tag.set_href(&url);

    a_tag.click();

    web_sys::Url::revoke_object_url(&url)
        .ok()
        .ok_or("url revoke failed")?;

    Ok(())
}
