use leptos::prelude::*;

use crate::components::{
    creatures_panel::CreaturesPanel,
    history_panel::HistoryPanel,
    loadouts_panel::LoadoutsPanel,
    analyse_panel::AnalysePanel,
    crafting_panel::CraftingPanel,
    settings_panel::SettingsPanel,
    run_panel::RunPanel,
    mu_panel::MuPanel,
    pnl_panel::PnlPanel,
    stock_panel::StockPanel,
};

// ─── Tab-Enum ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Run,
    Creatures,
    Loadouts,
    History,
    Analyse,
    Crafting,
    Pnl,
    Mu,
    Stock,
    Settings,
}

impl Tab {
    fn label(self) -> &'static str {
        match self {
            Tab::Run       => "🎯 Run",
            Tab::Creatures => "🐾 Kreaturen",
            Tab::Loadouts  => "🔫 Loadouts",
            Tab::History   => "📊 History",
            Tab::Analyse   => "📈 Analyse",
            Tab::Crafting  => "🔨 Crafting",
            Tab::Pnl       => "💰 P&L",
            Tab::Mu        => "📦 Materialien",
            Tab::Stock     => "🏭 Lager",
            Tab::Settings  => "⚙️ Einstellungen",
        }
    }

    fn all() -> &'static [Tab] {
        &[
            Tab::Run, Tab::Creatures, Tab::Loadouts,
            Tab::History, Tab::Analyse, Tab::Crafting, Tab::Pnl, Tab::Mu, Tab::Stock, Tab::Settings,
        ]
    }
}

// Context: ist der Run-Tab gerade sichtbar? RunPanel abonniert das zum Daten-Refresh.
#[derive(Clone, Copy)]
pub struct RunTabActive(pub RwSignal<bool>);

// ─── AppShell ────────────────────────────────────────────────────────────────

#[component]
pub fn AppShell() -> impl IntoView {
    let active_tab = RwSignal::new(Tab::Run);

    let run_tab_active = RwSignal::new(true);
    provide_context(RunTabActive(run_tab_active));
    Effect::new(move |_| {
        run_tab_active.set(active_tab.get() == Tab::Run);
    });

    view! {
        <div style="min-height:100vh">
            // Tab-Leiste
            <nav class="tab-bar" style="position:sticky;top:0;z-index:10;background:#f8fafc;padding:0 16px;">
                {Tab::all().iter().map(|&tab| {
                    let active_tab = active_tab;
                    view! {
                        <button
                            class=move || if active_tab.get() == tab { "tab active" } else { "tab" }
                            on:click=move |_| active_tab.set(tab)
                        >
                            {tab.label()}
                        </button>
                    }
                }).collect_view()}
            </nav>

            // RunPanel IMMER gemountet (CSS-hidden beim Tab-Wechsel) → Run-Zustand bleibt erhalten
            <main style="padding:16px">
                <div style=move || if active_tab.get() == Tab::Run { "" } else { "display:none" }>
                    <RunPanel />
                </div>
                <Show when=move || active_tab.get() == Tab::Creatures>
                    <CreaturesPanel />
                </Show>
                <Show when=move || active_tab.get() == Tab::Loadouts>
                    <LoadoutsPanel />
                </Show>
                <Show when=move || active_tab.get() == Tab::History>
                    <HistoryPanel />
                </Show>
                <Show when=move || active_tab.get() == Tab::Analyse>
                    <AnalysePanel />
                </Show>
                <Show when=move || active_tab.get() == Tab::Crafting>
                    <CraftingPanel />
                </Show>
                <Show when=move || active_tab.get() == Tab::Pnl>
                    <PnlPanel />
                </Show>
                <Show when=move || active_tab.get() == Tab::Mu>
                    <MuPanel />
                </Show>
                <Show when=move || active_tab.get() == Tab::Stock>
                    <StockPanel />
                </Show>
                <Show when=move || active_tab.get() == Tab::Settings>
                    <SettingsPanel />
                </Show>
            </main>
        </div>
    }
}
