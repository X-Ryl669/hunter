use termion::event::Key;

use std::error::Error;
use std::io::Write;
use std::sync::{Arc, Mutex};

use crate::coordinates::{Coordinates};
use crate::files::{File, Files};
use crate::listview::ListView;
use crate::miller_columns::MillerColumns;
use crate::widget::Widget;
use crate::tabview::{TabView, Tabbable};
use crate::preview::WillBeWidget;
use crate::fail::HResult;

#[derive(PartialEq)]
pub struct FileBrowser {
    pub columns: MillerColumns<WillBeWidget<ListView<Files>>>,
    pub cwd: File
}

impl Tabbable for TabView<FileBrowser> {
    fn new_tab(&mut self) {
        let tab = FileBrowser::new().unwrap();
        self.push_widget(tab);
        self.active += 1;
    }

    fn close_tab(&mut self) {
        self.close_tab_();
    }

    fn next_tab(&mut self) {
        self.next_tab_();
    }

    fn get_tab_names(&self) -> Vec<Option<String>> {
        self.widgets.iter().map(|filebrowser| {
            let path = filebrowser.cwd.path();
            let last_dir = path.components().last().unwrap();
            let dir_name = last_dir.as_os_str().to_string_lossy().to_string();
            Some(dir_name)
        }).collect()
    }

    fn active_tab(& self) -> & dyn Widget {
        self.active_tab_()
    }

    fn active_tab_mut(&mut self) -> &mut dyn Widget {
        self.active_tab_mut_()
    }

    fn on_next_tab(&mut self) {
        self.active_tab_mut().refresh();
    }

    fn on_key_sub(&mut self, key: Key) {
        match key {
            Key::Char('!') => {
                let tab_dirs = self.widgets.iter().map(|w| w.cwd.clone())
                                                  .collect::<Vec<_>>();
                self.widgets[self.active].exec_cmd(tab_dirs).ok();
            }
            _ => self.active_tab_mut().on_key(key)
        }
    }
}

impl FileBrowser {
    pub fn new() -> Result<FileBrowser, Box<Error>> {
        let cwd = std::env::current_dir().unwrap();
        let coords = Coordinates::new_at(crate::term::xsize(),
                                         crate::term::ysize() - 2,
                                         1,
                                         2);

        let mut miller = MillerColumns::new();
        miller.set_coordinates(&coords);


        let (_, main_coords, _) = miller.calculate_coordinates();

        let main_path: std::path::PathBuf = cwd.ancestors().take(1).map(|path| std::path::PathBuf::from(path)).collect();
        let main_widget = WillBeWidget::new(Box::new(move |_| {
            let mut list = ListView::new(Files::new_from_path(&main_path).unwrap());
            list.set_coordinates(&main_coords);
            list.animate_slide_up();
            Ok(list)
        }));

        miller.push_widget(main_widget);


        let cwd = File::new_from_path(&cwd).unwrap();

        Ok(FileBrowser { columns: miller,
                         cwd: cwd })
    }

    pub fn enter_dir(&mut self) -> HResult<()> {
        let file = self.selected_file()?;
        let (_, coords, _) = self.columns.calculate_coordinates();

        match file.read_dir() {
            Ok(files) => {
                std::env::set_current_dir(&file.path).unwrap();
                self.cwd = file.clone();
                let view = WillBeWidget::new(Box::new(move |_| {
                    let files = files.clone();
                    let mut list = ListView::new(files);
                    list.set_coordinates(&coords);
                    list.animate_slide_up();
                    Ok(list)
                }));
                self.columns.push_widget(view);
            },
            _ => {
                let status = std::process::Command::new("rifle")
                    .args(file.path.file_name())
                    .status();

                match status {
                    Ok(status) =>
                        self.show_status(&format!("\"{}\" exited with {}",
                                                  "rifle", status)),
                    Err(err) =>
                        self.show_status(&format!("Can't run this \"{}\": {}",
                                                  "rifle", err))

                }
            }
        }
        Ok(())
    }

    pub fn go_back(&mut self) -> HResult<()> {
        self.columns.pop_widget();

        self.refresh();
        Ok(())
    }

    pub fn update_preview(&mut self) -> HResult<()> {
        let file = self.selected_file()?.clone();
        let preview = &mut self.columns.preview;
        preview.set_file(&file);
        Ok(())
    }

    pub fn fix_selection(&mut self) -> HResult<()> {
        let cwd = self.cwd()?;
        (*self.left_widget()?.lock()?).as_mut()?.select_file(&cwd);
        Ok(())
    }

    pub fn fix_left(&mut self) -> HResult<()> {
        if self.left_widget().is_err() {
            let file = self.selected_file()?.clone();
            if let Some(grand_parent) = file.grand_parent() {
                let (coords, _, _) = self.columns.calculate_coordinates();
                let left_view = WillBeWidget::new(Box::new(move |_| {
                    let mut view
                        = ListView::new(Files::new_from_path(&grand_parent)?);
                    view.set_coordinates(&coords);
                    Ok(view)
                }));
                self.columns.prepend_widget(left_view);
            }
        }
        Ok(())
    }

    pub fn cwd(&self) -> HResult<File> {
        let widget = self.columns.get_main_widget()?.widget()?;
        let cwd = (*widget.lock()?).as_ref()?.content.directory.clone();
        Ok(cwd)
    }

    pub fn set_cwd(&mut self) -> HResult<()> {
        let cwd = self.cwd()?;
        std::env::set_current_dir(&cwd.path)?;
        self.cwd = cwd;
        Ok(())
    }

    pub fn selected_file(&self) -> HResult<File> {
        let widget = self.main_widget()?;
        let file = widget.lock()?.as_ref()?.selected_file().clone();
        Ok(file)
    }

    pub fn main_widget(&self) -> HResult<Arc<Mutex<Option<ListView<Files>>>>> {
        let widget = self.columns.get_main_widget()?.widget()?;
        Ok(widget)
    }

    pub fn left_widget(&self) -> HResult<Arc<Mutex<Option<ListView<Files>>>>> {
        let widget = self.columns.get_left_widget()?.widget()?;
        Ok(widget)
    }

    pub fn quit_with_dir(&self) -> HResult<()> {
        let cwd = self.cwd()?.path;
        let selected_file = self.selected_file()?;
        let selected_file = selected_file.path.to_string_lossy();

        let mut filepath = dirs_2::home_dir()?;
        filepath.push(".hunter_cwd");

        let output = format!("HUNTER_CWD=\"{}\"\nF=\"{}\"",
                             cwd.to_str()?,
                             selected_file);

        let mut file = std::fs::File::create(filepath)?;
        file.write(output.as_bytes())?;
        panic!("Quitting!");
        Ok(())
    }

    pub fn turbo_cd(&mut self) -> HResult<()> {
        let dir = self.minibuffer("cd: ");

        match dir {
            Ok(dir) => {
                self.columns.widgets.widgets.clear();
                let cwd = File::new_from_path(&std::path::PathBuf::from(&dir))?;
                self.cwd = cwd;
                let dir = std::path::PathBuf::from(&dir);
                let left_dir = std::path::PathBuf::from(&dir);
                let (left_coords, main_coords, _) = self.columns.calculate_coordinates();

                let middle = WillBeWidget::new(Box::new(move |_| {
                    let files = Files::new_from_path(&dir.clone())?;
                    let mut listview = ListView::new(files);
                    listview.set_coordinates(&main_coords);
                    Ok(listview)
                }));
                let left = WillBeWidget::new(Box::new(move |_| {
                    let files = Files::new_from_path(&left_dir.parent()?)?;
                    let mut listview = ListView::new(files);
                    listview.set_coordinates(&left_coords);
                    Ok(listview)
                }));
                self.columns.push_widget(left);
                self.columns.push_widget(middle);
            },
            Err(_) => {}
        }
        Ok(())
    }

    fn exec_cmd(&mut self, tab_dirs: Vec<File>) -> HResult<()> {
        let widget = self.left_widget()?;
        let widget = widget.lock()?;
        let selected_files = (*widget).as_ref()?.content.get_selected();

        let file_names
            = selected_files.iter().map(|f| f.name.clone()).collect::<Vec<String>>();

        let cmd = self.minibuffer("exec:")?;

        self.show_status(&format!("Running: \"{}\"", &cmd));

        let filename = self.selected_file()?.name.clone();

        let mut cmd = if file_names.len() == 0 {
            cmd.replace("$s", &format!("{}", &filename))
        } else {
            let args = file_names.iter().map(|f| {
                format!(" \"{}\" ", f)
            }).collect::<String>();
            let clean_cmd = cmd.replace("$s", "");

            clean_cmd + &args
        };

        for (i, tab_dir) in tab_dirs.iter().enumerate() {
            let tab_identifier = format!("${}", i);
            let tab_path = tab_dir.path.to_string_lossy();
            cmd = cmd.replace(&tab_identifier, &tab_path);
        }

        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .status();
        let mut bufout = std::io::BufWriter::new(std::io::stdout());
        write!(bufout, "{}{}",
               termion::style::Reset,
               termion::clear::All).unwrap();

        match status {
            Ok(status) => self.show_status(&format!("\"{}\" exited with {}",
                                                    cmd, status)),
            Err(err) => self.show_status(&format!("Can't run this \"{}\": {}",
                                                  cmd, err)),
        }
        Ok(())
    }
}

impl Widget for FileBrowser {
    fn get_coordinates(&self) -> &Coordinates {
        &self.columns.coordinates
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) {
        self.columns.coordinates = coordinates.clone();
        self.refresh();
    }
    fn render_header(&self) -> String {
        if self.main_widget().is_err() { return "".to_string() }
        let xsize = self.get_coordinates().xsize();
        let file = self.selected_file().unwrap();
        let name = &file.name;

        let color = if file.is_dir() || file.color.is_none() {
            crate::term::highlight_color() } else {
            crate::term::from_lscolor(file.color.as_ref().unwrap()) };

        let path = file.path.parent().unwrap().to_string_lossy().to_string();

        let pretty_path = format!("{}/{}{}", path, &color, name );
        let sized_path = crate::term::sized_string(&pretty_path, xsize);
        sized_path
    }
    fn render_footer(&self) -> String {
        if self.main_widget().is_err() { return "".to_string() }
        let xsize = self.get_coordinates().xsize();
        let ypos = self.get_coordinates().position().y();
        let file = self.selected_file().unwrap();

        let permissions = file.pretty_print_permissions().unwrap_or("NOPERMS".into());
        let user = file.pretty_user().unwrap_or("NOUSER".into());
        let group = file.pretty_group().unwrap_or("NOGROUP".into());
        let mtime = file.pretty_mtime().unwrap_or("NOMTIME".into());


        let selection = (*self.main_widget().as_ref().unwrap().lock().unwrap()).as_ref().unwrap().get_selection();
        let file_count = (*self.main_widget().unwrap().lock().unwrap()).as_ref().unwrap().content.len();
        let file_count = format!("{}", file_count);
        let digits = file_count.len();
        let file_count = format!("{:digits$}/{:digits$}",
                                 selection,
                                 file_count,
                                 digits = digits);
        let count_xpos = xsize - file_count.len() as u16;
        let count_ypos = ypos + self.get_coordinates().ysize();

        format!("{} {}:{} {} {} {}", permissions, user, group, mtime,
                crate::term::goto_xy(count_xpos, count_ypos), file_count)
     }
    fn refresh(&mut self) {
        self.update_preview().ok();
        self.fix_left().ok();
        self.fix_selection().ok();
        self.set_cwd().ok();
        self.columns.refresh();
    }

    fn get_drawlist(&self) -> String {
        if self.columns.get_left_widget().is_err() {
            self.columns.get_clearlist() + &self.columns.get_drawlist()
        } else {
            self.columns.get_drawlist()
        }
    }

    fn on_key(&mut self, key: Key) {
        match key {
            Key::Char('/') => { self.turbo_cd().ok(); },
            Key::Char('Q') => { self.quit_with_dir().ok(); },
            Key::Right | Key::Char('f') => { self.enter_dir().ok(); },
            Key::Left | Key::Char('b') => { self.go_back().ok(); },
            _ => self.columns.get_main_widget_mut().unwrap().on_key(key),
        }
        self.update_preview().ok();
    }
}
