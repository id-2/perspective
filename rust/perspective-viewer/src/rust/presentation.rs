// ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
// ┃ ██████ ██████ ██████       █      █      █      █      █ █▄  ▀███ █       ┃
// ┃ ▄▄▄▄▄█ █▄▄▄▄▄ ▄▄▄▄▄█  ▀▀▀▀▀█▀▀▀▀▀ █ ▀▀▀▀▀█ ████████▌▐███ ███▄  ▀█ █ ▀▀▀▀▀ ┃
// ┃ █▀▀▀▀▀ █▀▀▀▀▀ █▀██▀▀ ▄▄▄▄▄ █ ▄▄▄▄▄█ ▄▄▄▄▄█ ████████▌▐███ █████▄   █ ▄▄▄▄▄ ┃
// ┃ █      ██████ █  ▀█▄       █ ██████      █      ███▌▐███ ███████▄ █       ┃
// ┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫
// ┃ Copyright (c) 2017, the Perspective Authors.                              ┃
// ┃ ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌ ┃
// ┃ This file is part of the Perspective library, distributed under the terms ┃
// ┃ of the [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0). ┃
// ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛

use std::cell::RefCell;
use std::ops::Deref;
use std::rc::Rc;

use async_lock::Mutex;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::*;
use yew::html::ImplicitClone;

use crate::utils::*;

/// The available themes as detected in the browser environment or set
/// explicitly when CORS prevents detection.  Detection is expensive and
/// typically must be performed only once, when `document.styleSheets` is
/// up-to-date.
#[derive(Clone)]
pub struct Presentation(Rc<PresentationHandle>);

impl PartialEq for Presentation {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Deref for Presentation {
    type Target = PresentationHandle;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ImplicitClone for Presentation {}

pub struct PresentationHandle {
    viewer_elem: HtmlElement,
    theme_data: Mutex<ThemeData>,
    name: RefCell<Option<String>>,
    is_settings_open: RefCell<bool>,
    is_workspace: RefCell<Option<bool>>,
    pub settings_open_changed: PubSub<bool>,
    pub column_settings_open_changed: PubSub<(bool, Option<String>)>,
    pub column_settings_updated: PubSub<JsValue>,
    pub theme_config_updated: PubSub<(Vec<String>, Option<usize>)>,
    pub title_changed: PubSub<Option<String>>,
}

#[derive(Default)]
pub struct ThemeData {
    themes: Option<Vec<String>>,
}

impl Presentation {
    pub fn new(elem: &HtmlElement) -> Self {
        let theme = Self(Rc::new(PresentationHandle {
            viewer_elem: elem.clone(),
            name: Default::default(),
            theme_data: Default::default(),
            settings_open_changed: Default::default(),
            column_settings_open_changed: Default::default(),
            column_settings_updated: Default::default(),
            is_settings_open: Default::default(),
            is_workspace: Default::default(),
            theme_config_updated: PubSub::default(),
            title_changed: PubSub::default(),
        }));

        ApiFuture::spawn(theme.clone().init());
        theme
    }

    pub fn get_title(&self) -> Option<String> {
        self.name.borrow().clone()
    }

    pub fn set_title(&self, title: Option<String>) {
        *self.name.borrow_mut() = title.clone();
        self.title_changed.emit(title);
    }

    pub fn get_is_workspace(&self) -> bool {
        if self.is_workspace.borrow().is_none() {
            let is_workspace = self
                .viewer_elem
                .parent_element()
                .map(|x| x.tag_name() == "PERSPECTIVE-WORKSPACE")
                .unwrap_or_default();

            *self.is_workspace.borrow_mut() = Some(is_workspace);
        }

        self.is_workspace.borrow().unwrap()
    }

    pub fn set_settings_attribute(&self, opt: bool) {
        self.viewer_elem
            .toggle_attribute_with_force("settings", opt)
            .unwrap();
    }

    pub fn is_settings_open(&self) -> bool {
        *self.is_settings_open.borrow()
    }

    pub fn set_settings_open(&self, open: Option<bool>) -> ApiResult<bool> {
        let open_state = open.unwrap_or_else(|| !*self.is_settings_open.borrow());
        if *self.is_settings_open.borrow() != open_state {
            *self.is_settings_open.borrow_mut() = open_state;
            self.set_settings_attribute(open_state);
            self.settings_open_changed.emit(open_state);
        }

        Ok(open_state)
    }

    async fn init(self) -> ApiResult<()> {
        self.set_theme_attribute(self.get_selected_theme_name().await.as_deref())
    }

    /// Get the available theme names from the browser environment by parsing
    /// readable stylesheets.  This method is memoized - the state can be
    /// flushed by calling `reset()`.
    pub async fn get_available_themes(&self) -> ApiResult<Vec<String>> {
        let mut data = self.0.theme_data.lock().await;
        if data.themes.is_none() {
            await_dom_loaded().await?;
            let themes = get_theme_names(&self.0.viewer_elem)?;
            data.themes = Some(themes);
        }

        Ok(data.themes.clone().unwrap())
    }

    /// Reset the state.  `styleSheets` will be re-parsed next time
    /// `get_themes()` is called if the `themes` argument is `None`.
    pub async fn reset_available_themes(&self, themes: Option<Vec<String>>) {
        let mut mutex = self.0.theme_data.lock().await;
        mutex.themes = themes;
    }

    pub async fn get_selected_theme_config(&self) -> ApiResult<(Vec<String>, Option<usize>)> {
        let themes = self.get_available_themes().await?;
        let name = self.0.viewer_elem.get_attribute("theme");
        let index = name
            .and_then(|x| themes.iter().position(|y| y == &x))
            .or(if !themes.is_empty() { Some(0) } else { None });

        Ok((themes, index))
    }

    /// Returns the currently applied theme, or the default theme if no theme
    /// has been set and themes are detected in the `document`, or `None` if
    /// no themes are available.
    pub async fn get_selected_theme_name(&self) -> Option<String> {
        let (themes, index) = self.get_selected_theme_config().await.ok()?;
        index.and_then(|x| themes.get(x).cloned())
    }

    fn set_theme_attribute(&self, theme: Option<&str>) -> ApiResult<()> {
        if let Some(theme) = theme {
            Ok(self.0.viewer_elem.set_attribute("theme", theme)?)
        } else {
            Ok(self.0.viewer_elem.remove_attribute("theme")?)
        }
    }

    /// Set the theme by name, or `None` for the default theme.
    pub async fn set_theme_name(&self, theme: Option<&str>) -> ApiResult<()> {
        let (themes, _) = self.get_selected_theme_config().await?;
        let index = if let Some(theme) = theme {
            self.set_theme_attribute(Some(theme))?;
            themes.iter().position(|x| x == theme)
        } else if !themes.is_empty() {
            self.set_theme_attribute(themes.get(0).map(|x| x.as_str()))?;
            Some(0)
        } else {
            self.set_theme_attribute(None)?;
            None
        };

        self.theme_config_updated.emit((themes, index));
        Ok(())
    }
}

macro_rules! iter_index {
    ($x:expr) => {
        (0..$x.length()).map(|x| $x.item(x))
    };
}

fn fill_rule_theme_names(
    themes: &mut Vec<String>,
    rule: &Option<CssRule>,
    elem: &HtmlElement,
) -> ApiResult<()> {
    if let Some(rule) = rule.as_apierror()?.dyn_ref::<CssStyleRule>() {
        let txt = rule.selector_text();
        if elem.matches(&txt)? {
            let style = rule.style();
            let x = (0..style.length()).map(|x| style.item(x));
            for property in x {
                if property == "--theme-name" {
                    let name = style.get_property_value("--theme-name")?;
                    let trimmed = name.trim();
                    themes.push(trimmed[1..trimmed.len() - 1].to_owned());
                }
            }
        }
    }

    Ok(())
}

fn fill_sheet_theme_names(
    themes: &mut Vec<String>,
    sheet: &Option<StyleSheet>,
    elem: &HtmlElement,
) -> ApiResult<()> {
    let sheet = sheet
        .as_ref()
        .into_apierror()?
        .unchecked_ref::<CssStyleSheet>();

    if let Ok(rules) = sheet.css_rules() {
        for rule in iter_index!(&rules) {
            fill_rule_theme_names(themes, &rule, elem).unwrap_or_default();
        }
    }

    Ok(())
}

/// Search the document's `styleSheets` for rules which apply to `elem` and
/// provide the `--theme-name` CSS custom property.
fn get_theme_names(elem: &HtmlElement) -> Result<Vec<String>, JsValue> {
    let doc = window().unwrap().document().unwrap();
    let sheets = doc.style_sheets();
    let mut themes: Vec<String> = vec![];
    for sheet in iter_index!(sheets) {
        fill_sheet_theme_names(&mut themes, &sheet, elem)?;
    }

    Ok(themes)
}
