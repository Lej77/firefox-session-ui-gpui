#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod elm;
mod host;

use crate::elm::{MsgSender, Update};
use gpui::{
    div, prelude::*, px, AlignItems, AnyView, App, AppContext, Application, AssetSource,
    ClipboardItem, Entity, Pixels, SharedString, Size, StyleRefinement, WeakEntity, Window,
    WindowOptions,
};
use gpui_component::{
    button::Button,
    checkbox::Checkbox,
    dropdown::{Dropdown, DropdownItem, DropdownState},
    group_box::GroupBox,
    h_flex,
    input::{InputState, TextInput},
    label::Label,
    list::{List, ListDelegate, ListItem},
    text::TextView,
    tooltip::Tooltip,
    v_flex, ContextModal, Icon, IconName, IndexPath, Root, StyledExt,
};
use rust_embed::RustEmbed;
use std::borrow::Cow;
use std::path::PathBuf;

/// An asset source that loads assets from the `./assets` folder.
#[derive(RustEmbed)]
#[folder = "./assets"]
#[include = "icons/**/*.svg"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }

        Self::get(path)
            .map(|f| Some(f.data))
            .ok_or_else(|| anyhow::anyhow!("could not find asset at path \"{path}\""))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter_map(|p| p.starts_with(path).then(|| p.into()))
            .collect())
    }
}

struct WizardList {
    parent: WeakEntity<FirefoxSessionUtility>,
    found_profiles: Vec<host::FirefoxProfileInfo>,
    selected_index: Option<gpui_component::IndexPath>,
}
impl ListDelegate for WizardList {
    type Item = ListItem;

    fn items_count(&self, _section: usize, _cx: &App) -> usize {
        self.found_profiles.len()
    }

    fn render_item(
        &self,
        ix: gpui_component::IndexPath,
        _window: &mut Window,
        _cx: &mut Context<gpui_component::List<Self>>,
    ) -> Option<Self::Item> {
        self.found_profiles.get(ix.row).map(|item| {
            ListItem::new(ix)
                .child(Label::new(item.name().into_owned()))
                .selected(Some(ix) == self.selected_index)
        })
    }

    fn set_selected_index(
        &mut self,
        ix: Option<gpui_component::IndexPath>,
        window: &mut Window,
        cx: &mut Context<gpui_component::List<Self>>,
    ) {
        self.selected_index = ix;
        cx.notify();

        let Some(ix) = ix else { return };
        let Some(selected) = self.found_profiles.get(ix.row) else {
            return;
        };
        let selected = selected
            .find_sessionstore_file()
            .to_string_lossy()
            .into_owned();

        if let Some(parent) = self.parent.upgrade() {
            parent.update(cx, |parent, cx| {
                parent.update(window, cx, Command::SetInputPath(selected, None));
                parent.update(window, cx, Command::LoadNewInputData);
            })
        }
        window.close_modal(cx);
    }
}

struct Wizard {
    list: Entity<List<WizardList>>,
}
impl Wizard {
    fn new(
        window: &mut Window,
        cx: &mut Context<Wizard>,
        parent: WeakEntity<FirefoxSessionUtility>,
    ) -> Self {
        let list = cx.new(|cx| {
            List::new(
                WizardList {
                    parent,
                    found_profiles: Vec::new(),
                    selected_index: None,
                },
                window,
                cx,
            )
            .no_query()
        });
        Wizard { list }
    }
    fn open_modal(window: &mut Window, cx: &mut App, view: WeakEntity<Wizard>) {
        let Ok(list) = view.read_with(cx, |wiz, _| wiz.list.clone()) else {
            return;
        };
        list.update(cx, |view, _cx| {
            view.delegate_mut().found_profiles = host::FirefoxProfileInfo::all_profiles();
        });
        window.open_modal(cx, move |modal, _window, _cx| {
            modal
                .my_10()
                .title("Select Firefox Session Data")
                .child(
                    v_flex()
                        .child("Firefox Profiles:")
                        .child(v_flex().child(list.clone()).h_64())
                        .child(Button::new("cancel").mt_8().label("Cancel").on_click({
                            move |_, window, cx| {
                                eprintln!("Modal closed via button");
                                window.close_modal(cx);
                            }
                        })),
                )
                .on_close(|_, _, _| {
                    eprintln!("Modal closed");
                })
        })
    }
}

/// A view of an output format.
#[derive(Clone, Copy, gpui::IntoElement)]
pub struct FormatInfoValue(pub host::FormatInfo);
impl DropdownItem for FormatInfoValue {
    type Value = host::FormatInfo;

    fn title(&self) -> SharedString {
        self.0.as_str().into()
    }

    fn value(&self) -> &Self::Value {
        &self.0
    }

    fn display_title(&self) -> Option<gpui::AnyElement> {
        Some(gpui::IntoElement::into_any_element(*self))
    }
}
impl gpui::RenderOnce for FormatInfoValue {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div()
            .size_full()
            .child(self.0.as_str())
            .id(SharedString::from(format!(
                "{}-output-format-option",
                self.0.as_str()
            )))
            .tooltip({
                let info = self.0;
                move |window, cx| {
                    Tooltip::element(move |window, cx| {
                        TextView::markdown(info.as_str(), info.to_string(), window, cx)
                    })
                    .build(window, cx)
                }
            })
    }
}

#[derive(Clone)]
pub struct TabGroupList {
    parent: WeakEntity<FirefoxSessionUtility>,
    tab_groups: host::AllTabGroups,
    selected_tab_groups: host::GenerateOptions,
    /// Most recently selected list item.
    selected_item: Option<IndexPath>,
}
impl TabGroupList {
    fn change_selected_tab_group(&mut self, index: u32, open: bool, select: bool) -> bool {
        let (mut indexes, mut other) = (
            &mut self.selected_tab_groups.open_group_indexes,
            &mut self.selected_tab_groups.closed_group_indexes,
        );
        if !open {
            std::mem::swap(&mut indexes, &mut other);
        }
        if select {
            let indexes = indexes.get_or_insert_with(Vec::new);
            other.get_or_insert_with(Vec::new);
            if !indexes.contains(&index) {
                indexes.push(index);
                true // regen
            } else {
                false // already selected
            }
        } else if let Some(indexes) = indexes {
            let len = indexes.len();
            indexes.retain(|v| *v != index);
            if indexes.len() != len {
                // Something was removed => update preview:
                if self.selected_tab_groups.selected_groups() == 0 {
                    // Nothing selected => select all open windows:
                    self.selected_tab_groups.open_group_indexes = None;
                    self.selected_tab_groups
                        .closed_group_indexes
                        .get_or_insert_with(Vec::new);
                }
                true // regen
            } else {
                false
            }
        } else {
            false // nothing to deselect
        }
    }
}
impl ListDelegate for TabGroupList {
    type Item = ListItem;

    fn sections_count(&self, _cx: &App) -> usize {
        2 // open and closed
    }

    fn items_count(&self, section: usize, _cx: &App) -> usize {
        match section {
            0 => self.tab_groups.open.len(),
            1 => self.tab_groups.closed.len(),
            _ => 0,
        }
    }

    fn render_item(
        &self,
        ix: IndexPath,
        _window: &mut Window,
        _cx: &mut Context<'_, List<Self>>,
    ) -> Option<Self::Item> {
        let (groups, selected_indexes) = match ix.section {
            0 => (
                &self.tab_groups.open,
                &self.selected_tab_groups.open_group_indexes,
            ),
            1 => (
                &self.tab_groups.closed,
                &self.selected_tab_groups.closed_group_indexes,
            ),
            _ => return None,
        };
        groups.get(ix.row).map(|item| {
            let is_selected = selected_indexes
                .as_ref()
                .is_some_and(|indexes| indexes.contains(&item.index));
            ListItem::new(ix)
                .child(Label::new(item.name.clone()))
                .check_icon(IconName::Check)
                .confirmed(is_selected)
                .selected(is_selected)
        })
    }

    fn render_section_header(
        &self,
        section: usize,
        _window: &mut Window,
        _cx: &mut Context<'_, List<Self>>,
    ) -> Option<impl IntoElement> {
        let title = match section {
            0 => "Open Windows",
            1 => "Closed Windows",
            _ => return None,
        };

        Some(
            h_flex()
                .px_2()
                .py_1()
                .gap_2()
                .text_sm()
                // .text_color(cx.theme().muted_foreground)
                .child(Icon::new(IconName::Folder))
                .child(title),
        )
    }

    fn render_section_footer(
        &self,
        _section: usize,
        _window: &mut Window,
        _cx: &mut Context<'_, List<Self>>,
    ) -> Option<impl IntoElement> {
        Some(div().px_2().py_1().child(""))
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _window: &mut Window,
        _cx: &mut Context<List<Self>>,
    ) {
        self.selected_item = ix;
    }

    fn confirm(&mut self, _secondary: bool, window: &mut Window, cx: &mut Context<List<Self>>) {
        let Some(ix) = self.selected_item else { return };
        let selected_indexes = match ix.section {
            0 => &self.selected_tab_groups.open_group_indexes,
            1 => &self.selected_tab_groups.closed_group_indexes,
            _ => return,
        };
        let was_selected = selected_indexes
            .as_ref()
            .is_some_and(|indexes| indexes.contains(&(ix.row as u32)));

        if self.change_selected_tab_group(ix.row as u32, ix.section == 0, !was_selected) {
            let parent = self.parent.clone();
            MsgSender::new(window.to_async(cx), parent)
                .spawn(async move |_window, mut sender| {
                    sender.send(Command::RegeneratePreview);
                })
                .detach();
        }

        cx.notify();
    }
}

#[derive(Clone)]
pub enum Command {
    SetInputPath(String, Option<rfd::FileHandle>),
    LoadNewInputData,
    UpdateLoadedData(host::FileInfo),
    ParsedTabGroups(host::AllTabGroups),
    RegeneratePreview,
    SetPreview(String),
    ChangeTabGroupSelection {
        open: bool,
        index: u32,
        select: bool,
    },
    SetSavePath(String),
    SetStatus(String),
    SaveLinksToFile,
}
impl Update<Command> for FirefoxSessionUtility {
    fn update(&mut self, window: &mut Window, cx: &mut Context<Self>, msg: Command) {
        match msg {
            Command::SetInputPath(input_path, data) => {
                self.new_input_data = data;
                self.new_input.update(cx, |new_input, cx| {
                    new_input.set_value(input_path, window, cx);
                })
            }
            Command::LoadNewInputData => {
                let input_path = self.new_input.read(cx).value();
                self.loaded_input.update(cx, |loaded_input, cx| {
                    loaded_input.set_value(input_path.clone(), window, cx);
                });

                let mut data = host::FileInfo::new(if let Some(data) = &self.new_input_data {
                    data.path().to_owned()
                } else {
                    PathBuf::from(input_path.as_str())
                });
                data.file_handle = self.new_input_data.clone();
                self.loaded_input_data = Some(data.clone());

                self.tab_group_list.update(cx, |tab_group_list, _cx| {
                    tab_group_list
                        .delegate_mut()
                        .selected_tab_groups
                        .open_group_indexes = None;
                    tab_group_list
                        .delegate_mut()
                        .selected_tab_groups
                        .closed_group_indexes = Some(Vec::new());
                });
                self.set_status(window, cx, "Reading input file");

                MsgSender::from_cx(window, cx)
                    .spawn(async move |_window, mut sender| {
                        if let Err(e) = data.load_data().await {
                            sender.send(Command::SetStatus(format!("Failed to read file: {e}")));
                            return;
                        };
                        sender.send(Command::UpdateLoadedData(data.clone()));
                        loop {
                            match &data.data {
                                Some(host::FileData::Compressed { .. }) => {
                                    sender
                                        .send(Command::SetStatus("Decompressing data".to_string()));
                                    if let Err(e) = data.decompress_data().await {
                                        sender.send(Command::SetStatus(format!(
                                            "Failed to decompress data: {e}"
                                        )));
                                        return;
                                    }
                                }
                                Some(host::FileData::Uncompressed { .. }) => {
                                    sender.send(Command::SetStatus(
                                        "Parsing session data".to_string(),
                                    ));
                                    if let Err(e) = data.parse_session_data().await {
                                        sender.send(Command::SetStatus(format!(
                                            "Failed to parse session data: {e}"
                                        )));
                                        return;
                                    }
                                }
                                Some(host::FileData::Parsed { .. }) => {
                                    sender.send(match data.get_groups_from_session(true).await {
                                        Ok(all_groups) => Command::ParsedTabGroups(all_groups),
                                        Err(e) => Command::SetStatus(format!(
                                            "Failed to list windows in session: {e}"
                                        )),
                                    });
                                    return;
                                }
                                None => unreachable!("we just loaded the data"),
                            }
                            sender.send(Command::UpdateLoadedData(data.clone()));
                        }
                    })
                    .detach();
            }
            Command::UpdateLoadedData(data) => {
                self.loaded_input_data = Some(data);
            }
            Command::ParsedTabGroups(all_groups) => {
                self.tab_group_list.update(cx, |tab_group_list, _cx| {
                    tab_group_list.delegate_mut().tab_groups = all_groups;
                });
                self.update(window, cx, Command::RegeneratePreview);
            }
            Command::RegeneratePreview => {
                let Some(data) = self.loaded_input_data.clone() else {
                    return;
                };
                let options = self
                    .tab_group_list
                    .read(cx)
                    .delegate()
                    .selected_tab_groups
                    .clone();

                self.set_status(window, cx, "Generating preview");
                MsgSender::from_cx(window, cx)
                    .spawn(async move |_window, mut sender| {
                        let cmd = match data.to_text_links(options).await {
                            Ok(preview) => Command::SetPreview(preview),
                            Err(e) => {
                                Command::SetStatus(format!("Failed to generate preview: {e}"))
                            }
                        };
                        sender.send(cmd);
                    })
                    .detach();
            }
            Command::SetPreview(v) => {
                self.preview.update(cx, |preview, cx| {
                    preview.set_value(v, window, cx);
                });
                self.set_status(window, cx, "Successfully loaded session data");
            }
            Command::ChangeTabGroupSelection { .. } => {
                // TODO: update sidebar list
            }
            Command::SetSavePath(v) => {
                self.output_path.update(cx, |output_path, cx| {
                    output_path.set_value(v, window, cx);
                });
            }
            Command::SetStatus(v) => {
                self.set_status(window, cx, v);
            }
            Command::SaveLinksToFile => {
                let Some(data) = self.loaded_input_data.clone() else {
                    return;
                };
                let save_path = PathBuf::from(self.output_path.read(cx).value().as_str());
                let selected = self
                    .tab_group_list
                    .read(cx)
                    .delegate()
                    .selected_tab_groups
                    .clone();
                let Some(output_format) = self.output_format.read(cx).selected_value() else {
                    return;
                };
                let output_options = host::OutputOptions {
                    format: *output_format,
                    overwrite: self.overwrite,
                    create_folder: self.create_folder,
                };

                self.set_status(window, cx, "Saving links to file");

                let view = cx.weak_entity();
                window
                    .spawn(cx, async move |window| {
                        let new_status = if let Err(e) =
                            data.save_links(save_path, selected, output_options).await
                        {
                            Command::SetStatus(format!("Failed to save links to file: {e}"))
                        } else {
                            Command::SetStatus("Successfully saved links to a file".to_owned())
                        };
                        MsgSender::new(window.clone(), view).send(new_status);
                    })
                    .detach();
            }
        }
    }
}

struct FirefoxSessionUtility {
    input_wizard: Entity<Wizard>,
    new_input: Entity<InputState>,
    new_input_data: Option<rfd::FileHandle>,
    loaded_input: Entity<InputState>,
    loaded_input_data: Option<host::FileInfo>,
    preview: Entity<InputState>,
    tab_group_list: Entity<List<TabGroupList>>,
    output_path: Entity<InputState>,
    create_folder: bool,
    overwrite: bool,
    output_format: Entity<DropdownState<Vec<FormatInfoValue>>>,
    status: Entity<InputState>,
}
impl FirefoxSessionUtility {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let new_input = cx.new(|cx: &mut Context<'_, _>| InputState::new(window, cx));
        let loaded_input = cx.new(|cx: &mut Context<'_, _>| InputState::new(window, cx));
        let input_wizard = cx.new({
            let parent = cx.weak_entity();
            |cx| Wizard::new(window, cx, parent)
        });
        let preview = cx.new(|cx: &mut Context<'_, _>| {
            InputState::new(window, cx).multi_line().searchable(true)
        });

        let tab_group_list = cx.new({
            let parent = cx.weak_entity();
            |cx| {
                List::new(
                    TabGroupList {
                        parent,
                        tab_groups: Default::default(),
                        selected_tab_groups: Default::default(),
                        selected_item: None,
                    },
                    window,
                    cx,
                )
                .no_query()
            }
        });

        let output_path = cx.new(|cx: &mut Context<'_, _>| {
            InputState::new(window, cx).default_value({
                #[cfg(windows)]
                {
                    std::env::var("USERPROFILE")
                        .map(|home| home + r"\Downloads\firefox-links")
                        .unwrap_or_default()
                }
                #[cfg(not(windows))]
                {
                    String::new()
                }
            })
        });

        let output_format = cx.new(|cx: &mut Context<'_, _>| {
            DropdownState::new(
                host::FormatInfo::all()
                    .iter()
                    .copied()
                    .map(FormatInfoValue)
                    .collect::<Vec<_>>(),
                host::FormatInfo::all()
                    .iter()
                    .position(|fmt| *fmt == host::FormatInfo::PDF)
                    .map(gpui_component::IndexPath::new),
                window,
                cx,
            )
        });
        let status = cx.new(|cx: &mut Context<'_, _>| InputState::new(window, cx));

        Self {
            new_input,
            new_input_data: None,
            loaded_input,
            loaded_input_data: None,
            input_wizard,
            preview,
            tab_group_list,
            create_folder: false,
            overwrite: false,
            output_path,
            output_format,
            status,
        }
    }

    pub fn set_status(
        &mut self,
        window: &mut Window,
        cx: &mut impl AppContext,
        new_status: impl Into<SharedString>,
    ) {
        self.status.update(cx, |status, cx| {
            status.set_value(new_status, window, cx);
        });
    }

    fn input_browse_event_listener(
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) {
        let sender = MsgSender::from_cx(window, cx);
        move |_, window, _cx| {
            /*
            let prompt =
                cx.prompt_for_paths(gpui::PathPromptOptions {
                    files: true,
                    directories: false,
                    multiple: false,
                    prompt: Some(
                        "Select Firefox Sessionstore File".into(),
                    ),
                });
            let prompt = async move {
                let mut selected = prompt.await.unwrap().unwrap()?;
                let first = selected.remove(0);
                assert_eq!(selected.len(), 0);
                Some(first)
            };
            // */
            let prompt = host::prompt_load_file(Some(&host::NoDisplayHandle(&*window)));
            let prompt = async move {
                let file = prompt.await?;
                Some(Command::SetInputPath(
                    file.path().to_string_lossy().into_owned(),
                    Some(file),
                ))
            };

            sender
                .spawn(async move |_window, mut sender| {
                    if let Some(command) = prompt.await {
                        sender.send(command);
                    }
                })
                .detach();
        }
    }

    fn output_browse_event_listener(
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) {
        let sender = MsgSender::from_cx(window, cx);
        move |_, window, _cx| {
            // let prompt =
            //     cx.prompt_for_new_path("".as_ref(), None);
            // let prompt = async move { prompt.await.unwrap().unwrap() };

            let prompt = host::prompt_save_file(Some(&host::NoDisplayHandle(&*window)));
            let prompt = async move {
                Some(Command::SetSavePath(
                    prompt.await?.path().to_string_lossy().into_owned(),
                ))
            };

            sender
                .spawn(async move |_window, mut sender| {
                    if let Some(command) = prompt.await {
                        sender.send(command);
                    }
                })
                .detach();
        }
    }

    /// Display info about the currently selected output format.
    fn output_format_tooltip(
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let view = cx.weak_entity();
        move |window, cx| {
            let output_format = view
                .upgrade()
                .and_then(|view| view.read(cx).output_format.read(cx).selected_value())
                .copied();
            let info = if let Some(output_format) = output_format {
                SharedString::from(output_format.to_string())
            } else {
                "No output format selected.".into()
            };
            Tooltip::element(move |window, cx| {
                TextView::markdown("output-format-tooltip", info.clone(), window, cx)
            })
            .build(window, cx)
        }
    }
}
impl Render for FirefoxSessionUtility {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let modal_layer = Root::render_modal_layer(window, cx);

        h_flex()
            .size_full()
            // Sidebar (select windows/groups):
            .child(
                div()
                    .flex()
                    //.bg(rgb(0x2e7d32))
                    .h_full()
                    .w(Pixels::from(250.0))
                    .justify_center()
                    .items_center()
                    .text_xl()
                    //.text_color(rgb(0xffffff))
                    .child(self.tab_group_list.clone()),
            )
            // Main view:
            .child(
                v_flex()
                    .p_2()
                    //.bg(rgb(0xff0032))
                    .size_full()
                    // Input options:
                    .child(
                        h_flex()
                            .my_2()
                            .child("Path to sessionstore file:")
                            .child(TextInput::new(&self.new_input).ml_2())
                            .child(
                                Button::new("input-wizard")
                                    .on_click({
                                        let view = self.input_wizard.downgrade();
                                        move |_, window, cx| {
                                            Wizard::open_modal(window, cx, view.clone());
                                        }
                                    })
                                    .child("Wizard")
                                    .ml_2(),
                            )
                            .child(
                                Button::new("input-browse")
                                    .on_click(Self::input_browse_event_listener(window, cx))
                                    .child("Browse")
                                    .ml_2(),
                            ),
                    )
                    .child(
                        h_flex()
                            .my_2()
                            .child("Current data was loaded from:")
                            .child(TextInput::new(&self.loaded_input).ml_2().disabled(true))
                            .child(
                                Button::new("input-load")
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.update(window, cx, Command::LoadNewInputData);
                                    }))
                                    .child("Load new data")
                                    .ml_2(),
                            ),
                    )
                    // Preview:
                    .child(Label::new("Tabs as links:").my_2())
                    .child(
                        TextInput::new(&self.preview)
                            .flex_grow()
                            .mb_2()
                            .disabled(true),
                    )
                    // Output options:
                    .child(
                        h_flex()
                            .my_2()
                            .child("File path to write links to:")
                            .child(TextInput::new(&self.output_path).ml_2())
                            .child(
                                Button::new("output-browse")
                                    .on_click(Self::output_browse_event_listener(window, cx))
                                    .child("Browse")
                                    .ml_2(),
                            ),
                    )
                    .child(
                        h_flex()
                            .my_2()
                            .child(
                                Checkbox::new("output-create-folder")
                                    .label("Create folder if it doesn't exist")
                                    .checked(self.create_folder)
                                    .on_click(cx.listener(|view, checked, _, cx| {
                                        view.create_folder = *checked;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Checkbox::new("output-overwrite")
                                    .ml_4()
                                    .label("Overwrite file if it already exists")
                                    .checked(self.overwrite)
                                    .on_click(cx.listener(|view, checked, _, cx| {
                                        view.overwrite = *checked;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        h_flex()
                            .my_2()
                            .refine_style(&StyleRefinement {
                                align_items: Some(AlignItems::Stretch),
                                ..Default::default()
                            })
                            .child(
                                v_flex().child(
                                    Button::new("copy-links-to-clipboard")
                                        .on_click(cx.listener(|view, _, _window, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                view.preview.read(cx).value().as_str().to_owned(),
                                            ));
                                        }))
                                        .child("Copy links to clipboard")
                                        .flex_grow(),
                                ),
                            )
                            .child(div().flex_grow())
                            .child(
                                div().child(
                                    GroupBox::new()
                                        .content_style(
                                            StyleRefinement::default().py_2().px_2().border_2(),
                                        )
                                        .outline()
                                        .child(
                                            v_flex()
                                                .child(
                                                    Label::new("Output format")
                                                        .text_center()
                                                        .mb_2(),
                                                )
                                                .child(
                                                    div()
                                                        .child(
                                                            Dropdown::new(&self.output_format)
                                                                .min_w(px(200.)),
                                                        )
                                                        .id("select-output-format")
                                                        .tooltip(Self::output_format_tooltip(
                                                            window, cx,
                                                        )),
                                                ),
                                        ),
                                ),
                            )
                            .child(
                                v_flex().child(
                                    Button::new("save-links-to-file")
                                        .ml_2()
                                        .on_click(cx.listener(|view, _, window, cx| {
                                            view.update(window, cx, Command::SaveLinksToFile);
                                        }))
                                        .child("Save links to file")
                                        .flex_grow(),
                                ),
                            ),
                    )
                    // Status bar:
                    .child(
                        div()
                            .flex()
                            .my_2()
                            .flex_row()
                            .child("Status:")
                            .child(TextInput::new(&self.status).ml_2().disabled(true)),
                    ),
            )
            // Render the modal layer on top of the app content
            .children(modal_layer)
    }
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let _rt_guard = rt.enter();

    Application::new().with_assets(Assets).run(|cx: &mut App| {
        cx.new(|cx: &mut Context<'_, ()>| {
            // This must be called before using any GPUI Component features.
            gpui_component::init(cx);

            cx.open_window(
                WindowOptions {
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Firefox Session Data Utility".into()),
                        ..Default::default()
                    }),
                    window_min_size: Some(Size::new(px(800.), px(600.))),
                    ..Default::default()
                },
                |window: &mut Window, cx: &mut App| {
                    // Uncomment next line to test a specific theme instead of using the system theme:
                    // gpui_component::Theme::change(gpui_component::ThemeMode::Light, Some(window), cx);

                    let main_ui =
                        cx.new(|cx: &mut Context<'_, _>| FirefoxSessionUtility::new(window, cx));
                    cx.new(|cx| Root::new(main_ui.into(), window, cx))
                },
            )
            .expect("Failed to build and open window");
        });
    });
}
