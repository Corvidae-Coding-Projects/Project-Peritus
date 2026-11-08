//! Legal menu states and complete navigation transitions, independent of request dispatch.
use super::{AppModel, KeyCode, KeyEvent, ModelRole, destination::ModelDestination};
use peritus_app_protocol::{
    ProductModelCatalog, ProductModelChoice, ProductModelEffort, ProductRoleModels,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelStep {
    Agent,
    Model,
    Effort,
}

#[derive(Clone, Copy)]
pub(super) enum MenuStart {
    Agents,
    Models,
    CatalogRefresh,
}
pub(super) enum MenuAction {
    None,
    Discover,
    Refresh,
    Save(ModelRole, ProductModelChoice),
    Cancel,
    CatalogChanged,
    Warning(String),
}

#[derive(Debug)]
enum Page {
    Agent(ModelRole),
    Model { role: ModelRole, index: usize },
    Effort { role: ModelRole, choice: ProductModelChoice, model_index: usize, index: usize },
}

#[derive(Debug)]
pub(in crate::model::chat) struct ModelSetup {
    destination: ModelDestination,
    page: Page,
}

impl ModelSetup {
    pub(super) fn start(model: &AppModel, start: MenuStart) -> Option<(Self, MenuAction)> {
        let mut menu = Self {
            destination: ModelDestination::capture(model)?,
            page: Page::Agent(model.chat.model_role),
        };
        let action = match start {
            MenuStart::Agents => MenuAction::None,
            MenuStart::Models | MenuStart::CatalogRefresh => {
                menu.choose_agent(&model.chat.models, model.chat.catalog.as_ref());
                if matches!(start, MenuStart::CatalogRefresh) {
                    MenuAction::Refresh
                } else {
                    MenuAction::Discover
                }
            }
        };
        Some((menu, action))
    }
    pub(super) const fn step(&self) -> ModelStep {
        match self.page {
            Page::Agent(_) => ModelStep::Agent,
            Page::Model { .. } => ModelStep::Model,
            Page::Effort { .. } => ModelStep::Effort,
        }
    }
    pub(super) const fn role(&self) -> ModelRole {
        match self.page {
            Page::Agent(role) | Page::Model { role, .. } | Page::Effort { role, .. } => role,
        }
    }
    pub(super) const fn choice(&self) -> Option<&ProductModelChoice> {
        if let Page::Effort { choice, .. } = &self.page { Some(choice) } else { None }
    }
    pub(super) const fn selection(&self) -> usize {
        match self.page {
            Page::Agent(_) => 0,
            Page::Model { index, .. } | Page::Effort { index, .. } => index,
        }
    }
    pub(super) fn is_current(&self, model: &AppModel) -> bool {
        self.destination.matches_menu(model, self.role())
    }

    pub(super) fn catalog_replaced(
        &mut self,
        previous: Option<&ProductModelCatalog>,
        next: &ProductModelCatalog,
        models: &ProductRoleModels,
    ) {
        if let Page::Effort { choice, model_index, .. } = &mut self.page {
            *model_index = next
                .models()
                .iter()
                .position(|m| m.id() == choice.id())
                .unwrap_or_else(|| (*model_index).min(next.models().len().saturating_sub(1)));
            return;
        }
        let Page::Model { role, index } = &mut self.page else { return };
        *index = previous
            .filter(|p| p.profile() == next.profile())
            .and_then(|p| p.models().get(*index))
            .and_then(|selected| next.models().iter().position(|m| m.id() == selected.id()))
            .or_else(|| next.models().iter().position(|m| m.id() == role.choice(models).id()))
            .unwrap_or(0);
    }

    fn choose_agent(&mut self, models: &ProductRoleModels, catalog: Option<&ProductModelCatalog>) {
        let role = self.role();
        let index = catalog
            .filter(|c| c.profile() == self.destination.provider(role))
            .and_then(|c| c.models().iter().position(|m| m.id() == role.choice(models).id()))
            .unwrap_or(0);
        self.page = Page::Model { role, index };
    }
    fn back(&mut self) -> MenuAction {
        self.page = match self.page {
            Page::Agent(_) => return MenuAction::Cancel,
            Page::Model { role, .. } => Page::Agent(role),
            Page::Effort { role, model_index, .. } => Page::Model { role, index: model_index },
        };
        MenuAction::None
    }

    pub(super) fn key(
        &mut self,
        key: KeyEvent,
        models: &ProductRoleModels,
        catalog: Option<&ProductModelCatalog>,
    ) -> MenuAction {
        if matches!(key.code, KeyCode::Esc | KeyCode::Backspace) {
            return self.back();
        }
        match self.step() {
            ModelStep::Agent => self.agent_key(key, models, catalog),
            ModelStep::Model => self.model_key(key, models, catalog),
            ModelStep::Effort => self.effort_key(key, catalog),
        }
    }
    fn agent_key(
        &mut self,
        key: KeyEvent,
        models: &ProductRoleModels,
        catalog: Option<&ProductModelCatalog>,
    ) -> MenuAction {
        let mut role = self.role();
        let mut select = false;
        match key.code {
            KeyCode::Char('1' | '2' | '3') => {
                role = match key.code {
                    KeyCode::Char('1') => ModelRole::Writer,
                    KeyCode::Char('2') => ModelRole::Reviewer,
                    _ => ModelRole::Fixer,
                };
                select = true;
            }
            KeyCode::Enter => select = true,
            KeyCode::Up => role = role.next().next(),
            KeyCode::Down | KeyCode::Tab => role = role.next(),
            KeyCode::Home => role = ModelRole::Writer,
            KeyCode::End => role = ModelRole::Fixer,
            _ => {}
        }
        self.page = Page::Agent(role);
        if select {
            self.choose_agent(models, catalog);
            MenuAction::Discover
        } else {
            MenuAction::None
        }
    }
    fn model_key(
        &mut self,
        key: KeyEvent,
        models: &ProductRoleModels,
        catalog: Option<&ProductModelCatalog>,
    ) -> MenuAction {
        let role = self.role();
        let mut index = self.selection();
        let catalog = catalog.filter(|c| c.profile() == self.destination.provider(role));
        let last = catalog.map_or(0, |c| c.models().len().saturating_sub(1));
        match key.code {
            KeyCode::Char('r') => return MenuAction::Refresh,
            KeyCode::Up => index = index.saturating_sub(1),
            KeyCode::Down => index = (index + 1).min(last),
            KeyCode::Home => index = 0,
            KeyCode::End => index = last,
            KeyCode::Enter | KeyCode::Char('e') => {
                if let Some(entry) = catalog.and_then(|c| c.models().get(index)) {
                    match ProductModelChoice::new(entry.id().to_owned(), false) {
                        Ok(choice) => {
                            let choice = choice.with_effort(role.choice(models).effort());
                            let selected = ProductModelEffort::ALL
                                .iter()
                                .position(|e| *e == choice.effort())
                                .unwrap_or(0);
                            self.page =
                                Page::Effort { role, choice, model_index: index, index: selected };
                        }
                        Err(error) => return MenuAction::Warning(error.to_string()),
                    }
                }
                return MenuAction::None;
            }
            _ => {}
        }
        self.page = Page::Model { role, index };
        MenuAction::None
    }
    fn effort_key(&mut self, key: KeyEvent, catalog: Option<&ProductModelCatalog>) -> MenuAction {
        let Page::Effort { role, choice, model_index, index } = &mut self.page else {
            return MenuAction::None;
        };
        match key.code {
            KeyCode::Up => *index = index.saturating_sub(1),
            KeyCode::Down => *index = (*index + 1).min(ProductModelEffort::ALL.len() - 1),
            KeyCode::Home => *index = 0,
            KeyCode::End => *index = ProductModelEffort::ALL.len() - 1,
            KeyCode::Enter => {
                if !catalog.is_some_and(|c| {
                    c.profile() == self.destination.provider(*role)
                        && c.models().iter().any(|m| m.id() == choice.id())
                }) {
                    self.page = Page::Model { role: *role, index: *model_index };
                    return MenuAction::CatalogChanged;
                }
                return MenuAction::Save(
                    *role,
                    choice.clone().with_effort(ProductModelEffort::ALL[*index]),
                );
            }
            _ => {}
        }
        MenuAction::None
    }
}
