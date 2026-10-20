// SimGameVersion.cs -- which game build is running, for the bridge envelope.
//
// The advisor builds its engine for the build the mod reports
// (`game_version`, PROTOCOL.md section 1). The running build is read from the
// game's own release_info.json through ReleaseInfoManager, the same source the
// game's version label and Sentry tags use: `ReleaseInfoManager.Instance
// .ReleaseInfo?.Version` is `"v0.107.1"` / `"v0.111.0"`, identical API in both
// decomps (Core.Debug/ReleaseInfoManager.cs, ReleaseInfo.cs).
//
// The DLL itself is compiled for ONE build (SpireMentor.csproj `GameVersion`):
// several sts2 members the mod calls changed signature between v0.107.1 and
// v0.111.0 (`RunRngSet.Seed` uint -> ulong, `Rng.Counter` removed,
// `CardSelectCmd.UseSelector` gained a parameter), so a build for one game
// throws MissingMethodException on the other (tests/compile/RefCheck lists
// them). A mismatch is logged loudly at startup.

using System;
using MegaCrit.Sts2.Core.Debug;

namespace SpireMentor;

internal static class SimGameVersion
{
    /// <summary>The game build this DLL was compiled against.</summary>
    public const string BuiltFor =
#if GAME_V0111
        "v0.111.0";
#else
        "v0.107.1";
#endif

    private static string? _wire;

    /// <summary>
    /// The running game's build in wire form (<c>"v0.111.0"</c>), resolved
    /// once. Falls back to <see cref="BuiltFor"/> when release_info.json is
    /// missing or unreadable (a dev build of the game): the DLL only runs on
    /// the build it was compiled for, so that is the best available answer.
    /// </summary>
    public static string Wire => _wire ??= Resolve();

    /// <summary>Resolve and log the version (MainFile.Initialize, main thread).</summary>
    public static void LogAtStartup()
    {
        Logger.Log($"Game version: {Wire} (mod built for {BuiltFor})");
    }

    private static string Resolve()
    {
        string? raw = null;
        try
        {
            raw = ReleaseInfoManager.Instance.ReleaseInfo?.Version;
        }
        catch (Exception ex)
        {
            Logger.Log($"[SimGameVersion] Could not read the game's release info: {ex.Message}");
        }
        string? running = GameVersionFormat.Normalize(raw);
        if (running == null)
        {
            Logger.Log($"[SimGameVersion] release_info.json has no version; reporting the build target {BuiltFor}.");
            return BuiltFor;
        }
        if (running != BuiltFor)
        {
            Logger.Log(
                $"[SimGameVersion] WARNING: this Spire Mentor DLL was built for {BuiltFor} but the game is {running}. " +
                $"Parts of the mod will fail on this build; rebuild with -p:GameVersion={running}.");
        }
        return running;
    }
}
