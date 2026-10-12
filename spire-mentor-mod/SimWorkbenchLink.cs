// SimWorkbenchLink.cs -- the toolbar's bridge to the workbench web app.
//
// The workbench (sts2-pycli: FastAPI on :8077 + React SPA) is the mod's
// "separate window": logs, search trees and the run/branch tree render in a
// browser instead of ever-growing in-game overlays. The mod's only jobs are
// (a) know the URL and (b) open the default browser on it; starting or
// managing the Python side is the advisor process's business (its
// `--workbench auto` mode spawns one in dev installs), never the mod's.

using System;
using System.Net.Http;
using System.Threading.Tasks;
using Godot;
using MegaCrit.Sts2.Core.Helpers;

namespace SpireMentor;

internal static class SimWorkbenchLink
{
    /// <summary>The page to open; configurable (SpireMentorModConfig.WorkbenchUrl).</summary>
    public static string Url =>
        string.IsNullOrWhiteSpace(SpireMentorModConfig.WorkbenchUrl)
            ? "http://127.0.0.1:8077/?follow=1"
            : SpireMentorModConfig.WorkbenchUrl.Trim();

    /// <summary>
    /// Open the workbench in the default browser. Probes the server first
    /// (off the main thread) so a dead workbench produces a helpful note
    /// instead of a browser error page. Never throws, never blocks.
    /// </summary>
    public static void Open() => TaskHelper.RunSafely(OpenAsync());

    private static async Task OpenAsync()
    {
        string url = Url;
        bool alive = await ProbeAsync(url);
        if (!alive)
        {
            SimActionLog.Note(
                "Workbench is not reachable — start it with `just workbench` in sts2-pycli " +
                "(or turn the AI coach on: the advisor auto-starts it in dev installs). " +
                $"URL: {url}");
            Logger.Log($"[SimWorkbenchLink] Health probe failed for {url}; not opening a browser.");
            return;
        }
        // NOT OS.ShellOpen: that spawns the browser as a CHILD OF THE GAME,
        // and Steam waits for the game's whole descendant tree on quit -- a
        // browser opened this way left Steam stuck on "closing" until the
        // browser exited (observed 2026-08-21, Firefox). explorer.exe
        // delegates to the long-lived shell instead -- but it cannot parse a
        // raw URL with a query string (it opens a folder), so the URL goes
        // through an InternetShortcut file, which the shell resolves to the
        // default browser with the query intact.
        try
        {
            string dir = System.IO.Path.Combine(
                System.Environment.GetFolderPath(System.Environment.SpecialFolder.LocalApplicationData),
                "SpireMentor");
            System.IO.Directory.CreateDirectory(dir);
            string shortcut = System.IO.Path.Combine(dir, "workbench.url");
            System.IO.File.WriteAllText(shortcut,
                "[InternetShortcut]\r\nURL=" + url + "\r\n");
            using var p = System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo
            {
                FileName = "explorer.exe",
                Arguments = "\"" + shortcut + "\"",
                UseShellExecute = false,
            });
            SimActionLog.Note($"Opened workbench: {url}");
        }
        catch (Exception ex)
        {
            Logger.Log($"[SimWorkbenchLink] Open failed: {ex.Message}");
            SimActionLog.Note($"Could not open the browser. Workbench URL: {url}");
        }
    }

    private static async Task<bool> ProbeAsync(string url)
    {
        try
        {
            var baseUri = new Uri(url);
            var health = new Uri($"{baseUri.Scheme}://{baseUri.Authority}/api/health");
            using var http = new System.Net.Http.HttpClient { Timeout = TimeSpan.FromMilliseconds(1500) };
            using HttpResponseMessage resp = await http.GetAsync(health);
            return resp.IsSuccessStatusCode;
        }
        catch
        {
            return false;
        }
    }
}
