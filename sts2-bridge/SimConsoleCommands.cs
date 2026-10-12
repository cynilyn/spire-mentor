// SimConsoleCommands.cs -- `spirecoach` dev-console command.
//
// The game's DevConsole auto-registers AbstractConsoleCmd subtypes from mod
// assemblies (DevConsole.cs:33, ReflectionHelper.GetSubtypesInMods). The
// console itself is available whenever mods are loaded
// (ModManager.IsRunningModded()). DebugOnly=false so the command registers
// even outside Autopilot, where IsReleaseGame() reports a release build.
//
// This is the only control surface that can touch the Autopilot wait/anim
// multipliers -- deliberately not on the settings page, so a persisted UI
// value can never silently retune the sweep lane.

using System;
using System.Globalization;
using MegaCrit.Sts2.Core.DevConsole;
using MegaCrit.Sts2.Core.DevConsole.ConsoleCommands;
using MegaCrit.Sts2.Core.Entities.Players;

namespace SpireCoach;

public class SpireCoachConsoleCmd : AbstractConsoleCmd
{
    public override string CmdName => "spirecoach";

    public override string Args => "[mode off|advisor] [speed <0.1-4.0>] [hud on|off] [log] [waitmult <f>] [animmult <f>]";

    public override string Description => "Spire Coach control: show status, switch mode, set game speed / HUD / autopilot multipliers.";

    public override bool IsNetworked => false;

    public override bool DebugOnly => false;

    public override CmdResult Process(Player? issuingPlayer, string[] args)
    {
        if (args.Length == 0)
        {
            return new CmdResult(success: true,
                $"mode={SimRuntime.Mode} speed={SimSpeed.Current:0.0}x hud={(SpireCoachModConfig.ShowHud ? "on" : "off")} " +
                $"waitmult={SimRuntime.WaitMultiplier:0.###} animmult={SimRuntime.AnimMultiplier:0.###}");
        }

        string sub = args[0].ToLowerInvariant();
        string? value = args.Length > 1 ? args[1] : null;
        switch (sub)
        {
            case "mode":
                return SetMode(value);
            case "speed":
                if (!TryParseFloat(value, out float speed))
                {
                    return new CmdResult(success: false, "usage: spirecoach speed <0.1-4.0>");
                }
                SimSpeed.Apply(speed);
                return new CmdResult(success: true, $"speed -> {SimSpeed.Current:0.0}x");
            case "hud":
                if (value is not ("on" or "off"))
                {
                    return new CmdResult(success: false, "usage: spirecoach hud on|off");
                }
                SimActions.SetHud(value == "on");
                return new CmdResult(success: true, $"hud -> {value}");
            case "log":
                SimActions.ToggleLog();
                return new CmdResult(success: true, "toggled action-log panel");
            case "waitmult":
                if (!TryParseFloat(value, out float wait) || wait <= 0f)
                {
                    return new CmdResult(success: false, "usage: spirecoach waitmult <positive float>");
                }
                SimRuntime.WaitMultiplier = wait;
                return new CmdResult(success: true, $"waitmult -> {wait:0.###} (applies while Autopilot)");
            case "animmult":
                if (!TryParseFloat(value, out float anim) || anim <= 0f)
                {
                    return new CmdResult(success: false, "usage: spirecoach animmult <positive float>");
                }
                SimRuntime.AnimMultiplier = anim;
                return new CmdResult(success: true, $"animmult -> {anim:0.###} (applies while Autopilot)");
            default:
                return new CmdResult(success: false, $"unknown subcommand '{sub}'. {Args}");
        }
    }

    public override CompletionResult GetArgumentCompletions(Player? player, string[] args)
    {
        if (args.Length <= 1)
        {
            return CompleteArgument(
                ["mode", "speed", "hud", "log", "waitmult", "animmult"],
                [], args.Length == 1 ? args[0] : "");
        }
        if (args.Length == 2)
        {
            string[] candidates = args[0].ToLowerInvariant() switch
            {
                "mode" => ["off", "advisor"],
                "hud" => ["on", "off"],
                _ => [],
            };
            if (candidates.Length > 0)
            {
                return CompleteArgument(candidates, [args[0]], args[1]);
            }
        }
        return base.GetArgumentCompletions(player, args);
    }

    private static CmdResult SetMode(string? value)
    {
        // Leaving Autopilot stops the slayer rather than being refused --
        // `auto_start=true` persists in SpireCoach.conf between a sweep and a
        // human session, and the old refusal left the player stuck with the
        // AI's game modifiers.
        string? target = value?.ToLowerInvariant();
        // Only a valid target stops the slayer -- a typo must not silently
        // end an autopilot session.
        string note = (target == "off" || target == "advisor") && MainFile.StopAutopilot()
            ? " (autopilot stopped)"
            : "";
        switch (target)
        {
            case "off":
                SimRuntime.Mode = SimMode.Off;
                return new CmdResult(success: true, $"mode -> Off{note}");
            case "advisor":
                SimRuntime.Mode = SimMode.Advisor;
                return new CmdResult(success: true, $"mode -> Advisor{note}");
            case "autopilot":
                return new CmdResult(success: false,
                    "Autopilot cannot be entered at runtime (needs auto_start=true in SpireCoach.conf at launch).");
            default:
                return new CmdResult(success: false, "usage: spirecoach mode off|advisor");
        }
    }

    private static bool TryParseFloat(string? value, out float result)
    {
        result = 0f;
        return value != null &&
               float.TryParse(value, NumberStyles.Float, CultureInfo.InvariantCulture, out result);
    }
}
