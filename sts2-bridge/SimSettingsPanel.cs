// SimSettingsPanel.cs -- mod settings, reachable MID-RUN from the toolbar.
//
// BaseLib deliberately disables its "Mod Configuration" entry while a run is
// in progress (ModConfigPatch: "supporting in-run config will likely need
// work"), and re-enabling it would change BaseLib behavior for every mod.
// So the mod carries its own small panel instead, built from the SAME
// backing store (SimBridgeModConfig) and the same BaseLib NConfig* controls
// (they pull their visuals from the game's own pck, bind the static
// property, and refresh themselves on ConfigReloaded) -- the main-menu page,
// hotkeys, console and this panel can never disagree.
//
// Unlike the rest of the mod's overlays this panel's background is
// MouseFilter.Stop: it is an explicitly opened, explicitly closed dialog,
// so blocking the game underneath it is the expected behavior (same
// rationale as the action-log's header). While its line edits have focus
// the game suppresses our hotkeys (NHotkeyManager checks the focus owner);
// closing the panel releases focus.
//
// Persistence: BaseLib only saves configs on settings-submenu close and on
// quit, so this panel hooks ConfigChanged -> SaveDebounced while it exists
// (a crash mid-run must not lose edits).

using System;
using Godot;
using MegaCrit.Sts2.Core.Helpers;
using MegaCrit.Sts2.Core.Nodes;

namespace SimBridge;

internal sealed class SimSettingsPanel
{
    public static readonly SimSettingsPanel Instance = new();

    private SimSettingsPanel()
    {
    }

    private CanvasLayer? _layer;
    private PanelContainer? _panel;

    /// <summary>Toggle the panel (toolbar gear button). Safe from any thread.</summary>
    public void Toggle()
    {
        RunOnMainThread(() =>
        {
            if (!EnsureBuilt() || _panel == null)
            {
                return;
            }
            bool opening = !_panel.Visible;
            if (opening)
            {
                // Park below the toolbar strip, wherever it currently sits.
                float top = (SimToolbar.Instance.BottomY() ?? 60f) + 8f;
                _panel.Position = new Vector2(16f, top);
            }
            else
            {
                // Editing a LineEdit suppresses the game's hotkey dispatch;
                // never leave focus behind on close.
                _panel.GetViewport()?.GuiReleaseFocus();
            }
            _panel.Visible = opening;
        });
    }

    private bool EnsureBuilt()
    {
        if (_layer != null && GodotObject.IsInstanceValid(_layer))
        {
            return true;
        }
        try
        {
            if (Engine.GetMainLoop() is not SceneTree tree)
            {
                return false;
            }

            var layer = new CanvasLayer
            {
                Name = "SimBridgeSettingsLayer",
                Layer = 102,
            };

            var panel = new PanelContainer
            {
                Name = "SimBridgeSettingsPanel",
                Position = new Vector2(16f, 68f),
                CustomMinimumSize = new Vector2(520f, 0f),
                MouseFilter = Control.MouseFilterEnum.Stop,
                Visible = false,
            };
            panel.AddThemeStyleboxOverride("panel", SimUiKit.PanelStyle());

            var rows = new VBoxContainer
            {
                Name = "SettingsRows",
            };
            rows.AddThemeConstantOverride("separation", 4);

            var title = new HBoxContainer();
            title.AddThemeConstantOverride("separation", 8);
            Label titleLabel = SimUiKit.SmallLabel("SimBridge settings", Colors.White, 16);
            titleLabel.SizeFlagsHorizontal = Control.SizeFlags.ExpandFill;
            title.AddChild(titleLabel);
            title.AddChild(SimUiKit.MakeButton("✕", "Close", Toggle));
            rows.AddChild(title);

            if (!SimBridgeModConfig.BuildLiveRowsInto(rows))
            {
                // The NConfig* controls lean on the game's scene paths; if
                // that ever breaks, say where the working page is instead of
                // rendering an empty dialog.
                rows.AddChild(SimUiKit.SmallLabel(
                    "Settings UI unavailable (see godot.log).\n" +
                    "Use Options → Mod Configuration at the main menu instead.",
                    new Color(1f, 0.75f, 0.6f)));
            }

            panel.AddChild(rows);
            layer.AddChild(panel);
            tree.Root.AddChildSafely(layer);

            SimBridgeModConfig.EnsureLivePersistHook();

            _layer = layer;
            _panel = panel;
            Logger.Log("[SimSettingsPanel] Settings panel created");
            return true;
        }
        catch (Exception ex)
        {
            Logger.Log($"[SimSettingsPanel] Failed to create panel: {ex.Message}");
            _layer = null;
            _panel = null;
            return false;
        }
    }

    private static void RunOnMainThread(Action action)
    {
        try
        {
            if (NGame.IsMainThread())
            {
                Guarded(action);
            }
            else
            {
                Callable.From(() => Guarded(action)).CallDeferred();
            }
        }
        catch (Exception ex)
        {
            Logger.Log($"[SimSettingsPanel] Dispatch failed: {ex.Message}");
        }
    }

    private static void Guarded(Action action)
    {
        try
        {
            action();
        }
        catch (Exception ex)
        {
            Logger.Log($"[SimSettingsPanel] Ignoring UI error (never breaks gameplay): {ex.Message}");
        }
    }
}
