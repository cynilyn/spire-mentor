// SimActions.cs -- the mod's user-facing verbs, shared by every surface.
//
// Hotkeys (SimHotkeys), the toolbar (SimToolbar), the dev-console command
// (SimConsoleCommands) and the BaseLib settings page all funnel through
// here, so behavior (and the log lines users see) cannot drift between
// surfaces. Bodies moved verbatim from SimHotkeys (2026-08-21).

using System;
using Godot;

namespace SimBridge;

internal static class SimActions
{
    public static void ToggleHelp() => SimHelp.Toggle();

    public static void ToggleLog() => SimActionLog.TogglePanel();

    public static void NudgeSpeed(double offset) => SimSpeed.Nudge(offset);

    /// <summary>Copy this position to the clipboard for the workbench's
    /// live-state import (SimStateCopy).</summary>
    public static void CopyState() => SimStateCopy.Copy();

    /// <summary>Undo to the start of the current combat turn (replays the
    /// game's own combat tape -- SimCombatUndo).</summary>
    public static void UndoTurn() => SimCombatUndo.RequestUndoTurn();

    /// <summary>Rewind to the start of the current room (same seed, exact
    /// state -- the game's own room-entry save).</summary>
    public static void RestartRoom()
    {
        SimSnapshotMeta? target = SimSnapshots.CurrentRoomEntry();
        if (target == null)
        {
            SimActionLog.Note("Nothing to rewind to yet (the game saves at each room entry).");
            return;
        }
        SimRestore.RequestRestore(target.Id);
    }

    /// <summary>Rewind one room further back.</summary>
    public static void BackOneRoom()
    {
        SimSnapshotMeta? target = SimSnapshots.PreviousRoomEntry();
        if (target == null)
        {
            SimActionLog.Note("No earlier room snapshot in this run yet.");
            return;
        }
        SimRestore.RequestRestore(target.Id);
    }

    public static void ToggleHud() => SetHud(!SimBridgeModConfig.ShowHud);

    public static void SetHud(bool on)
    {
        SimBridgeModConfig.ShowHud = on;
        GD.Print($"[SimBridge] HUD {(on ? "shown" : "hidden")}");
        SimRuntime.NotifyChanged();
    }

    /// <summary>AI takes this one decision (F6 / toolbar).</summary>
    public static void AiOnce(string surface) =>
        RequireAdvisor(surface, () => SimAdvisor.Instance.ExecuteBestOnce());

    /// <summary>AI plays until stopped (F7 / toolbar), toggled.</summary>
    public static void ToggleAutoplay(string surface) =>
        RequireAdvisor(surface, () => SimAdvisor.Instance.ToggleAutoPlay());

    /// <summary>
    /// Off &lt;-&gt; Advisor; from Autopilot: stop the AI and hand the game
    /// back. (Autopilot can only be entered at launch, from the config file.)
    /// </summary>
    public static void ToggleAdvisor()
    {
        if (SimRuntime.Mode == SimMode.Autopilot)
        {
            // Taking over from the AI: stop the slayer FIRST so its card
            // selector and non-interactive mode are released before the mode
            // flips (see MainFile.StopAutopilot).
            MainFile.StopAutopilot();
            SimRuntime.Mode = SimMode.Advisor;
            GD.Print("[SimBridge] Autopilot stopped -> Advisor (you have the controls).");
            SimActionLog.Note("Autopilot stopped; you have the controls (Advisor mode).");
            return;
        }
        SimRuntime.Mode = SimRuntime.Mode == SimMode.Advisor ? SimMode.Off : SimMode.Advisor;
        GD.Print($"[SimBridge] Mode -> {SimRuntime.Mode}");
    }

    /// <summary>
    /// The takeover verbs only mean anything in Advisor mode; say so instead
    /// of looking broken (the advisor's own guards would silently no-op).
    /// <paramref name="what"/> names the surface the user pressed, e.g.
    /// "F6 (AI takes this decision)" or "the AI-once button".
    /// </summary>
    public static void RequireAdvisor(string what, Action action)
    {
        if (SimRuntime.Mode != SimMode.Advisor)
        {
            string how = SimRuntime.Mode == SimMode.Autopilot
                ? "the AI is already playing (Autopilot) -- press F8 to take over"
                : "press F8 to turn Advisor mode on";
            GD.Print($"[SimBridge] {what} needs Advisor mode: {how}.");
            SimActionLog.Note($"{what} needs Advisor mode: {how}.");
            return;
        }
        action();
    }
}
