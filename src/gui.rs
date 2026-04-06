// GUI implementation using iced framework

use crate::archiver::{Archiver, PackProgress};
use iced::widget::{
    button, column, container, progress_bar, row, scrollable, text, text_input, Column,
};
use iced::{
    executor, window, Alignment, Application, Command, Element, Length, Settings, Theme,
};
use rfd::FileDialog;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum Message {
    TabChanged(Tab),
    AddFiles,
    AddFolder,
    FilesSelected(Vec<PathBuf>),
    RemoveFile(usize),
    OutputNameChanged(String),
    PasswordChanged(String),
    StartPacking,
    PackingProgress(PackProgress),
    PackingComplete(Result<(), String>),
    SelectArchive,
    ArchiveSelected(Option<PathBuf>),
    SelectOutputDir,
    OutputDirSelected(Option<PathBuf>),
    UnpackPasswordChanged(String),
    StartUnpacking,
    UnpackingProgress(PackProgress),
    UnpackingComplete(Result<(), String>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tab {
    Pack,
    Unpack,
}

pub struct ArchiverApp {
    current_tab: Tab,
    // Pack mode state
    files_to_pack: Vec<PathBuf>,
    output_name: String,
    pack_password: String,
    packing_progress: Option<PackProgress>,
    pack_status: String,
    // Unpack mode state
    archive_path: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    unpack_password: String,
    unpacking_progress: Option<PackProgress>,
    unpack_status: String,
}

impl Default for ArchiverApp {
    fn default() -> Self {
        Self {
            current_tab: Tab::Pack,
            files_to_pack: Vec::new(),
            output_name: String::from("archive.rpak"),
            pack_password: String::new(),
            packing_progress: None,
            pack_status: String::new(),
            archive_path: None,
            output_dir: None,
            unpack_password: String::new(),
            unpacking_progress: None,
            unpack_status: String::new(),
        }
    }
}

impl Application for ArchiverApp {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (Self::default(), Command::none())
    }

    fn title(&self) -> String {
        String::from("Rust File Archiver")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::TabChanged(tab) => {
                self.current_tab = tab;
                Command::none()
            }
            Message::AddFiles => {
                let files = FileDialog::new().pick_files();
                if let Some(files) = files {
                    Command::perform(async move { files }, Message::FilesSelected)
                } else {
                    Command::none()
                }
            }
            Message::AddFolder => {
                let folder = FileDialog::new().pick_folder();
                if let Some(folder) = folder {
                    Command::perform(
                        async move { vec![folder] },
                        Message::FilesSelected,
                    )
                } else {
                    Command::none()
                }
            }
            Message::FilesSelected(mut files) => {
                self.files_to_pack.append(&mut files);
                Command::none()
            }
            Message::RemoveFile(index) => {
                if index < self.files_to_pack.len() {
                    self.files_to_pack.remove(index);
                }
                Command::none()
            }
            Message::OutputNameChanged(name) => {
                self.output_name = name;
                Command::none()
            }
            Message::PasswordChanged(pwd) => {
                self.pack_password = pwd;
                Command::none()
            }
            Message::StartPacking => {
                if self.files_to_pack.is_empty() {
                    self.pack_status = "Error: No files selected".to_string();
                    return Command::none();
                }

                let files = self.files_to_pack.clone();
                let output = PathBuf::from(&self.output_name);
                let password = if self.pack_password.is_empty() {
                    None
                } else {
                    Some(self.pack_password.clone())
                };

                self.pack_status = "Packing...".to_string();
                self.packing_progress = Some(PackProgress {
                    current_file: String::new(),
                    processed: 0,
                    total: 1,
                });

                Command::perform(
                    async move {
                        let progress = Arc::new(Mutex::new(None));
                        let progress_clone = progress.clone();

                        let result = Archiver::pack(
                            &files,
                            &output,
                            password.as_deref(),
                            move |p| {
                                *progress_clone.lock().unwrap() = Some(p);
                            },
                        );

                        result.map_err(|e| e.to_string())
                    },
                    Message::PackingComplete,
                )
            }
            Message::PackingProgress(progress) => {
                self.packing_progress = Some(progress);
                Command::none()
            }
            Message::PackingComplete(result) => {
                self.packing_progress = None;
                match result {
                    Ok(_) => {
                        self.pack_status = format!("✓ Successfully created {}", self.output_name);
                    }
                    Err(e) => {
                        self.pack_status = format!("✗ Error: {}", e);
                    }
                }
                Command::none()
            }
            Message::SelectArchive => {
                let file = FileDialog::new()
                    .add_filter("RPAK Archive", &["rpak"])
                    .pick_file();
                Command::perform(async move { file }, Message::ArchiveSelected)
            }
            Message::ArchiveSelected(path) => {
                self.archive_path = path;
                Command::none()
            }
            Message::SelectOutputDir => {
                let dir = FileDialog::new().pick_folder();
                Command::perform(async move { dir }, Message::OutputDirSelected)
            }
            Message::OutputDirSelected(path) => {
                self.output_dir = path;
                Command::none()
            }
            Message::UnpackPasswordChanged(pwd) => {
                self.unpack_password = pwd;
                Command::none()
            }
            Message::StartUnpacking => {
                let Some(archive) = &self.archive_path else {
                    self.unpack_status = "Error: No archive selected".to_string();
                    return Command::none();
                };

                let Some(output_dir) = &self.output_dir else {
                    self.unpack_status = "Error: No output directory selected".to_string();
                    return Command::none();
                };

                let archive = archive.clone();
                let output_dir = output_dir.clone();
                let password = if self.unpack_password.is_empty() {
                    None
                } else {
                    Some(self.unpack_password.clone())
                };

                self.unpack_status = "Unpacking...".to_string();
                self.unpacking_progress = Some(PackProgress {
                    current_file: String::new(),
                    processed: 0,
                    total: 1,
                });

                Command::perform(
                    async move {
                        let progress = Arc::new(Mutex::new(None));
                        let progress_clone = progress.clone();

                        let result = Archiver::unpack(
                            &archive,
                            &output_dir,
                            password.as_deref(),
                            move |p| {
                                *progress_clone.lock().unwrap() = Some(p);
                            },
                        );

                        result.map_err(|e| e.to_string())
                    },
                    Message::UnpackingComplete,
                )
            }
            Message::UnpackingProgress(progress) => {
                self.unpacking_progress = Some(progress);
                Command::none()
            }
            Message::UnpackingComplete(result) => {
                self.unpacking_progress = None;
                match result {
                    Ok(_) => {
                        self.unpack_status = "✓ Successfully unpacked archive".to_string();
                    }
                    Err(e) => {
                        self.unpack_status = format!("✗ Error: {}", e);
                    }
                }
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let tab_bar = row![
            button(text("Pack").size(16))
                .on_press(Message::TabChanged(Tab::Pack))
                .padding(12),
            button(text("Unpack").size(16))
                .on_press(Message::TabChanged(Tab::Unpack))
                .padding(12),
        ]
        .spacing(10);

        let content = match self.current_tab {
            Tab::Pack => self.pack_view(),
            Tab::Unpack => self.unpack_view(),
        };

        let main_column = column![tab_bar, content]
            .spacing(20)
            .padding(20)
            .width(Length::Fill)
            .height(Length::Fill);

        container(main_column)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }
}

impl ArchiverApp {
    fn pack_view(&self) -> Element<Message> {
        let mut file_list = Column::new().spacing(5);

        if self.files_to_pack.is_empty() {
            file_list = file_list.push(
                text("No files selected. Click 'Add Files' or 'Add Folder' to begin.")
                    .size(14)
                    .style(iced::theme::Text::Color(iced::Color::from_rgb(
                        0.6, 0.6, 0.6,
                    ))),
            );
        } else {
            for (idx, file) in self.files_to_pack.iter().enumerate() {
                let file_row = row![
                    text(file.display().to_string()).size(14),
                    button(text("✕").size(14))
                        .on_press(Message::RemoveFile(idx))
                        .padding(5),
                ]
                .spacing(10)
                .align_items(Alignment::Center);

                file_list = file_list.push(file_row);
            }
        }

        let file_list_scroll = scrollable(file_list)
            .height(Length::Fixed(200.0))
            .width(Length::Fill);

        let buttons = row![
            button(text("Add Files").size(14))
                .on_press(Message::AddFiles)
                .padding(10),
            button(text("Add Folder").size(14))
                .on_press(Message::AddFolder)
                .padding(10),
        ]
        .spacing(10);

        let output_row = row![
            text("Output:").size(14).width(Length::Fixed(80.0)),
            text_input("archive.rpak", &self.output_name)
                .on_input(Message::OutputNameChanged)
                .padding(8)
                .size(14),
        ]
        .spacing(10)
        .align_items(Alignment::Center);

        let password_row = row![
            text("Password:").size(14).width(Length::Fixed(80.0)),
            text_input("(optional)", &self.pack_password)
                .on_input(Message::PasswordChanged)
                .secure(true)
                .padding(8)
                .size(14),
        ]
        .spacing(10)
        .align_items(Alignment::Center);

        let pack_button = button(text("Pack Files").size(16))
            .on_press(Message::StartPacking)
            .padding(12)
            .width(Length::Fill);

        let mut content = column![
            text("Pack Mode").size(20),
            text("Select files and folders to compress").size(14),
            file_list_scroll,
            buttons,
            output_row,
            password_row,
            pack_button,
        ]
        .spacing(15)
        .width(Length::Fill);

        if let Some(progress) = &self.packing_progress {
            let progress_value = if progress.total > 0 {
                progress.processed as f32 / progress.total as f32
            } else {
                0.0
            };

            content = content.push(
                column![
                    progress_bar(0.0..=1.0, progress_value),
                    text(format!(
                        "{} ({}/{})",
                        progress.current_file, progress.processed, progress.total
                    ))
                    .size(12),
                ]
                .spacing(5),
            );
        }

        if !self.pack_status.is_empty() {
            content = content.push(text(&self.pack_status).size(14));
        }

        content.into()
    }

    fn unpack_view(&self) -> Element<Message> {
        let archive_display = if let Some(path) = &self.archive_path {
            path.display().to_string()
        } else {
            "No archive selected".to_string()
        };

        let archive_row = row![
            text("Archive:").size(14).width(Length::Fixed(80.0)),
            text(&archive_display).size(14),
            button(text("Browse").size(14))
                .on_press(Message::SelectArchive)
                .padding(8),
        ]
        .spacing(10)
        .align_items(Alignment::Center);

        let output_display = if let Some(path) = &self.output_dir {
            path.display().to_string()
        } else {
            "No output directory selected".to_string()
        };

        let output_row = row![
            text("Output:").size(14).width(Length::Fixed(80.0)),
            text(&output_display).size(14),
            button(text("Browse").size(14))
                .on_press(Message::SelectOutputDir)
                .padding(8),
        ]
        .spacing(10)
        .align_items(Alignment::Center);

        let password_row = row![
            text("Password:").size(14).width(Length::Fixed(80.0)),
            text_input("(if encrypted)", &self.unpack_password)
                .on_input(Message::UnpackPasswordChanged)
                .secure(true)
                .padding(8)
                .size(14),
        ]
        .spacing(10)
        .align_items(Alignment::Center);

        let unpack_button = button(text("Unpack Archive").size(16))
            .on_press(Message::StartUnpacking)
            .padding(12)
            .width(Length::Fill);

        let mut content = column![
            text("Unpack Mode").size(20),
            text("Select a .rpak archive to extract").size(14),
            archive_row,
            output_row,
            password_row,
            unpack_button,
        ]
        .spacing(15)
        .width(Length::Fill);

        if let Some(progress) = &self.unpacking_progress {
            let progress_value = if progress.total > 0 {
                progress.processed as f32 / progress.total as f32
            } else {
                0.0
            };

            content = content.push(
                column![
                    progress_bar(0.0..=1.0, progress_value),
                    text(format!(
                        "{} ({}/{})",
                        progress.current_file, progress.processed, progress.total
                    ))
                    .size(12),
                ]
                .spacing(5),
            );
        }

        if !self.unpack_status.is_empty() {
            content = content.push(text(&self.unpack_status).size(14));
        }

        content.into()
    }
}

pub fn run() -> iced::Result {
    ArchiverApp::run(Settings {
        window: window::Settings {
            size: iced::Size::new(700.0, 600.0),
            ..Default::default()
        },
        ..Default::default()
    })
}
