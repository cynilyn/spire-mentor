// SimAiPartner.cs -- AI co-op partners inside the host's own game process
// (docs/plans/2026-11-05-ai-coop-partners.md).
//
// An AI partner is a virtual peer: a real NetClientGameService that joins the
// host's lobby like any remote client, through the game's own JoinFlow, over
// the in-memory transport (SimAiPartnerTransport). It runs no simulation of
// its own. Every peer simulates every player, so the host already holds the
// partner's whole state; the partner reads its Player off the host's
// RunManager and only SENDS the messages a remote client sends for its own
// player (SimAiPartnerBrain). The host cannot tell it from a human on another
// PC.
//
// A session (SimAiPartnerSession) is one hosted lobby + run with partners:
// started from the multiplayer host menu's "With AI partners" button
// (SimAiPartnerMenu) or, for the unattended test lane, by
// `--spire-mentor-ai-partners N` at the main menu. It ends when the host's
// net service disconnects (leaving the lobby, the run ending).
//
// The version split: v0.107.1's JoinFlow builds its own NetClientGameService
// (parameterless constructors), v0.111.0's takes one built with the local
// PeerVersionInfo (a v0.111.0-only type). One DLL ships for both, so the
// constructors are reached by reflection.

using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Threading.Tasks;
using Godot;
using HarmonyLib;
using MegaCrit.Sts2.Core.AutoSlay.Helpers;
using MegaCrit.Sts2.Core.Entities.Multiplayer;
using MegaCrit.Sts2.Core.Helpers;
using MegaCrit.Sts2.Core.Models;
using MegaCrit.Sts2.Core.Multiplayer;
using MegaCrit.Sts2.Core.Multiplayer.Game;
using MegaCrit.Sts2.Core.Multiplayer.Messages.Lobby;
using MegaCrit.Sts2.Core.Multiplayer.Serialization;
using MegaCrit.Sts2.Core.Nodes.GodotExtensions;
using MegaCrit.Sts2.Core.Nodes.Screens.CharacterSelect;
using MegaCrit.Sts2.Core.Nodes.Screens.MainMenu;
using MegaCrit.Sts2.Core.Runs;
using GameLogging = MegaCrit.Sts2.Core.Logging;

namespace SpireMentor;

internal sealed class SimAiPartner
{
    /// <summary>The null platform names these "Test Client 1..3"
    /// (NullPlatformUtilStrategy); SimAiPartnerNames renames them. None
    /// collides with the host's id 1.</summary>
    internal static readonly ulong[] NetIds = { 1000uL, 2000uL, 3000uL };

    private const double TickSeconds = 0.25;

    private static bool _loopsInstalled;

    /// <summary>The partners of the active session (none outside one).</summary>
    public static IReadOnlyList<SimAiPartner> All =>
        (IReadOnlyList<SimAiPartner>?)SimAiPartnerSession.Active?.Partners ?? Array.Empty<SimAiPartner>();

    public static bool IsPartner(ulong netId) => All.Any(p => p.NetId == netId);

    public ulong NetId { get; }
    public int Seat { get; }
    public string Label { get; }

    /// <summary>The partner's own client service; null until JoinFlow has
    /// connected it.</summary>
    public INetGameService? Service { get; private set; }

    public bool Joined { get; private set; }

    /// <summary>The character the partner picked in the lobby.</summary>
    public CharacterModel? Character { get; private set; }

    public SimAiPartnerBrain Brain { get; }

    /// <summary>The partner's own advisor process (phase 2); null before the
    /// session starts it or off Windows. Without one the brain plays its
    /// first-legal fallback.</summary>
    public SimAiPartnerAdvisor? Advisor { get; private set; }

    public void StartAdvisor()
    {
        if (SimMp.AiPartnerNoAdvisor)
        {
            Logger.Log($"[SimAiPartner] {Label}: --spire-mentor-ai-partner-no-advisor; first-legal play.");
            return;
        }
        Advisor = new SimAiPartnerAdvisor(this);
        Advisor.Start();
    }

    private readonly string? _characterName;
    private object? _flow;

    public SimAiPartner(int seat, string? characterName)
    {
        Seat = seat;
        NetId = NetIds[seat];
        _characterName = characterName;
        Label = $"AI partner {seat + 1}";
        Brain = new SimAiPartnerBrain(this);
    }

    /// <summary>Once at mod start: the Harmony observers, the menu button,
    /// the per-frame update and the decision tick. All idle without a
    /// session.</summary>
    public static void Install(Harmony patcher)
    {
        try
        {
            SimAiPartnerPatches.Install(patcher);
            SimAiPartnerMenu.Install(patcher);
            SimAiPartnerNames.Install(patcher);
            if (Engine.GetMainLoop() is not SceneTree tree || _loopsInstalled)
            {
                return;
            }
            _loopsInstalled = true;
            var timer = new Godot.Timer { Name = "SpireMentorAiPartnerTick", WaitTime = TickSeconds, Autostart = true, OneShot = false };
            timer.Connect(Godot.Timer.SignalName.Timeout, Callable.From(TickAll));
            tree.Root.AddChildSafely(timer);
            // Each partner's service must be updated every frame, as the
            // game updates its own (JoinFlow / NGame update loops).
            tree.Connect(SceneTree.SignalName.ProcessFrame, Callable.From(UpdateAll));
            if (SimMp.AiPartnersOverride > 0)
            {
                TaskHelper.RunSafely(SimAiPartnerSession.StartFromCommandLineAsync(tree));
            }
        }
        catch (Exception ex)
        {
            Logger.Log($"[SimAiPartner] Could not install: {ex}");
        }
    }

    private static void UpdateAll()
    {
        foreach (SimAiPartner p in All)
        {
            if (!p.Joined || p.Service == null)
            {
                continue;
            }
            try
            {
                p.Service.Update();
            }
            catch (Exception ex)
            {
                Logger.Log($"[SimAiPartner] {p.Label} update: {ex.Message}");
            }
        }
    }

    private static void TickAll()
    {
        foreach (SimAiPartner p in All)
        {
            if (!p.Joined || p.Service == null || !p.Service.IsConnected)
            {
                continue;
            }
            try
            {
                p.Brain.Tick();
            }
            catch (Exception ex)
            {
                Logger.Log($"[SimAiPartner] {p.Label} tick: {ex}");
            }
        }
    }

    public async Task JoinAsync(SimLoopbackHost host, SceneTree tree)
    {
        try
        {
            Logger.Log($"[SimAiPartner] {Label} ({NetId}) joining the lobby.");
            (object flow, INetGameService? service) = CreateJoinFlow();
            _flow = flow;
            MethodInfo begin = flow.GetType().GetMethod("Begin")
                ?? throw new MissingMethodException("JoinFlow.Begin");
            // JoinFlow runs the service's update loop itself while joining,
            // and turns the network / action / sync log channels up to
            // (very) debug for the rest of the process (it restores them
            // only on failure); put them back after.
            var levels = new Dictionary<GameLogging.LogType, GameLogging.LogLevel>(
                GameLogging.Logger.logLevelTypeMap);
            var task = (Task)begin.Invoke(flow, new object?[] { SimLoopbackConnector.Create(host, NetId), tree })!;
            try
            {
                await task;
            }
            finally
            {
                foreach (var (type, level) in levels)
                {
                    GameLogging.Logger.logLevelTypeMap[type] = level;
                }
            }
            // v0.107.1: the flow built the service; read it back.
            service ??= flow.GetType().GetProperty("NetService")?.GetValue(flow) as INetGameService;
            if (service == null)
            {
                throw new InvalidOperationException("JoinFlow left no net service");
            }
            if (SimAiPartnerSession.Active?.Host != host)
            {
                // The host left the lobby while we were joining.
                service.Disconnect(NetError.Quit);
                return;
            }
            Service = service;
            SimAiPartnerNoopHandlers.RegisterAll(service);
            Joined = true;
            Logger.Log($"[SimAiPartner] {Label} joined.");
            await Task.Delay(300);
            ChooseCharacterAndReady();
        }
        catch (Exception ex)
        {
            Logger.Log($"[SimAiPartner] {Label} could not join: {ex}");
        }
    }

    /// <summary>The host's session ended: the transport already dropped us;
    /// stop updating.</summary>
    public void Leave()
    {
        Joined = false;
        try
        {
            Service?.Disconnect(NetError.Quit, now: true);
        }
        catch (Exception)
        {
            // Already gone with the host.
        }
        Service = null;
        Advisor?.Stop();
        Advisor = null;
    }

    /// <summary>v0.111.0: <c>new JoinFlow(new NetClientGameService(
    /// PeerVersionInfo.LocalDefault()))</c>; v0.107.1: <c>new JoinFlow()</c>,
    /// which creates its service in Begin.</summary>
    private static (object flow, INetGameService? service) CreateJoinFlow()
    {
        Type flowType = typeof(JoinFlow);
        ConstructorInfo? withService = flowType.GetConstructors().FirstOrDefault(c => c.GetParameters().Length == 1);
        if (withService == null)
        {
            return (Activator.CreateInstance(flowType)!, null);
        }
        object? version = SimLoopbackHosting.LocalVersionInfo();
        object service = version != null
            ? Activator.CreateInstance(typeof(NetClientGameService), version)!
            : Activator.CreateInstance(typeof(NetClientGameService))!;
        return (withService.Invoke(new[] { service }), (INetGameService)service);
    }

    private void ChooseCharacterAndReady()
    {
        Character = PickCharacter();
        Logger.Log($"[SimAiPartner] {Label} picks {Character.Id.Entry} and readies.");
        Send(new LobbyPlayerChangedCharacterMessage { character = Character });
        Send(new LobbyPlayerSetReadyMessage { ready = true });
    }

    private CharacterModel PickCharacter()
    {
        List<CharacterModel> all = ModelDb.AllCharacters.ToList();
        if (!string.IsNullOrEmpty(_characterName))
        {
            CharacterModel? named = all.FirstOrDefault(c => c.Id.Entry.Contains(_characterName, StringComparison.OrdinalIgnoreCase));
            if (named != null)
            {
                return named;
            }
            Logger.Log($"[SimAiPartner] No character matches \"{_characterName}\".");
        }
        return all[(Seat + 1) % all.Count];
    }

    /// <summary>Sends one message as this partner, as a remote client would.</summary>
    public void Send<T>(T message) where T : INetMessage
    {
        if (Service == null || !Service.IsConnected)
        {
            Logger.Log($"[SimAiPartner] {Label}: not connected; dropped {message}.");
            return;
        }
        Service.SendMessage(message);
    }
}

/// <summary>One hosted lobby + run with AI partners.</summary>
internal sealed class SimAiPartnerSession
{
    /// <summary>The game's own host lobby size (NMultiplayerHostSubmenu).</summary>
    private const int MaxPlayers = 4;

    private static readonly TimeSpan MainMenuWait = TimeSpan.FromSeconds(90);
    private const string MainMenuNodePath = "/root/Game/RootSceneContainer/MainMenu";

    public static SimAiPartnerSession? Active { get; private set; }

    public NetHostGameService Service { get; }
    public SimLoopbackHost Host { get; }
    public List<SimAiPartner> Partners { get; }

    private SimAiPartnerSession(NetHostGameService service, SimLoopbackHost host, List<SimAiPartner> partners)
    {
        Service = service;
        Host = host;
        Partners = partners;
    }

    /// <summary>
    /// Hosts a co-op lobby on the in-memory transport and pushes the game's
    /// own character select as host, as NMultiplayerHostSubmenu.StartHostAsync
    /// does for ENet / Steam; the partners join once the lobby is up.
    /// </summary>
    public static bool Start(NSubmenuStack stack, int partnerCount, IReadOnlyList<string>? characters = null)
    {
        if (Active != null)
        {
            Logger.Log("[SimAiPartner] A session is already active; ending it first.");
            Active.End("restart");
        }
        partnerCount = Math.Clamp(partnerCount, 1, SimAiPartner.NetIds.Length);
        (NetHostGameService service, SimLoopbackHost host) = SimLoopbackHosting.Start();
        var partners = Enumerable.Range(0, partnerCount)
            .Select(i => new SimAiPartner(i, characters != null && i < characters.Count ? characters[i] : null))
            .ToList();
        var session = new SimAiPartnerSession(service, host, partners);
        Active = session;
        service.Disconnected += _ => session.End("host disconnected");
        SimAiPartnerPatches.SetHostResumesForPartners(true);
        // Early: an advisor takes a while to load its nets, and the first
        // fight is only a Neow away.
        foreach (SimAiPartner p in partners)
        {
            p.StartAdvisor();
        }

        NCharacterSelectScreen screen = stack.GetSubmenuType<NCharacterSelectScreen>();
        screen.InitializeMultiplayerAsHost(service, MaxPlayers);
        stack.Push(screen);
        Logger.Log($"[SimAiPartner] Hosting a co-op lobby with {partnerCount} AI partner(s) (in-memory transport).");

        if (Engine.GetMainLoop() is SceneTree tree)
        {
            TaskHelper.RunSafely(session.JoinWhenLobbyUpAsync(tree, screen));
        }
        return true;
    }

    private async Task JoinWhenLobbyUpAsync(SceneTree tree, NCharacterSelectScreen screen)
    {
        // The lobby registers its handlers in InitializeMultiplayerAsHost;
        // waiting for the screen keeps the joins visible to the player.
        for (int i = 0; i < 100 && Active == this && !screen.IsVisibleInTree(); i++)
        {
            await tree.ToSignal(tree, SceneTree.SignalName.ProcessFrame);
        }
        foreach (SimAiPartner p in Partners)
        {
            if (Active != this)
            {
                return;
            }
            await p.JoinAsync(Host, tree);
        }
        if (SimMp.AiPartnerShotPath is { Length: > 0 } shot && Active == this)
        {
            // The lane's check of the lobby (names, characters, ready).
            await Task.Delay(400);
            string path = System.IO.Path.ChangeExtension(shot, null) + "_lobby.png";
            Error err = tree.Root.GetViewport().GetTexture().GetImage().SavePng(path);
            Logger.Log($"[SimAiPartner] lobby screenshot -> {path} ({err}).");
        }
    }

    public void End(string why)
    {
        if (Active != this)
        {
            return;
        }
        Active = null;
        SimAiPartnerPatches.SetHostResumesForPartners(false);
        foreach (SimAiPartner p in Partners)
        {
            p.Leave();
        }
        Logger.Log($"[SimAiPartner] Session ended ({why}).");
    }

    /// <summary><c>--spire-mentor-ai-partners N</c>: the unattended test
    /// lane starts the same session from the main menu.</summary>
    public static async Task StartFromCommandLineAsync(SceneTree tree)
    {
        int count = SimMp.AiPartnersOverride;
        DateTime deadline = DateTime.UtcNow + MainMenuWait;
        NMainMenu? menu = null;
        while (DateTime.UtcNow < deadline)
        {
            menu = tree.Root.GetNodeOrNull<NMainMenu>(MainMenuNodePath);
            if (menu != null && menu.IsVisibleInTree() && menu.SubmenuStack != null)
            {
                break;
            }
            menu = null;
            await tree.ToSignal(tree, SceneTree.SignalName.ProcessFrame);
        }
        if (menu == null)
        {
            Logger.Log("[SimAiPartner] --spire-mentor-ai-partners: the main menu never showed.");
            return;
        }
        // Let the menu finish its own entrance before pushing over it.
        await Task.Delay(1500);
        string[] chars = (SimMp.AiPartnerCharacters ?? "").Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        NSubmenuStack stack = menu.SubmenuStack;
        if (count == 1 && chars.Length == 0 && await PressMenuButtonAsync(tree, stack))
        {
            return;
        }
        Logger.Log($"[SimAiPartner] --spire-mentor-ai-partners {count}: hosting from the main menu.");
        Callable.From(() => Start(stack, count, chars)).CallDeferred();
    }

    /// <summary>The lane's default (one partner, its own pick) goes through
    /// the real entry: open the host menu, optionally screenshot it
    /// (<c>--spire-mentor-ai-partner-shot PATH</c>), press the button.</summary>
    private static async Task<bool> PressMenuButtonAsync(SceneTree tree, NSubmenuStack stack)
    {
        try
        {
            var hostMenu = stack.GetSubmenuType<NMultiplayerHostSubmenu>();
            stack.Push(hostMenu);
            await Task.Delay(2000);
            if (SimMp.AiPartnerShotPath is { Length: > 0 } shot)
            {
                Image image = tree.Root.GetViewport().GetTexture().GetImage();
                Error err = image.SavePng(shot);
                Logger.Log($"[SimAiPartner] host menu screenshot -> {shot} ({err}).");
            }
            if (hostMenu.GetNodeOrNull<NSubmenuButton>("SpireMentorAiPartnersButton") is not { } button)
            {
                Logger.Log("[SimAiPartner] The host menu has no AI-partners button; hosting directly.");
                return false;
            }
            Logger.Log("[SimAiPartner] --spire-mentor-ai-partners 1: pressing the host menu's AI-partners button.");
            button.EmitSignal(NClickableControl.SignalName.Released, button);
            return true;
        }
        catch (Exception ex)
        {
            Logger.Log($"[SimAiPartner] Could not use the host menu: {ex.Message}; hosting directly.");
            return false;
        }
    }
}

/// <summary>
/// The host broadcasts every synced message to the partner's service too. A
/// message type with no handler makes the bus log an ERROR per message
/// (NetMessageBus.SendMessageToAllHandlers), so the partner listens to every
/// type and drops it: what it needs it reads off the host's state.
/// </summary>
internal static class SimAiPartnerNoopHandlers
{
    public static void RegisterAll(INetGameService service)
    {
        MethodInfo register = typeof(INetGameService).GetMethod(nameof(INetGameService.RegisterMessageHandler))!;
        MethodInfo noop = typeof(SimAiPartnerNoopHandlers).GetMethod(nameof(Noop), BindingFlags.NonPublic | BindingFlags.Static)!;
        int n = 0;
        foreach (Type t in typeof(INetMessage).Assembly.GetTypes())
        {
            if (t.IsInterface || t.IsAbstract || t.IsGenericTypeDefinition || !typeof(INetMessage).IsAssignableFrom(t))
            {
                continue;
            }
            try
            {
                Type delegateType = typeof(MessageHandlerDelegate<>).MakeGenericType(t);
                Delegate handler = Delegate.CreateDelegate(delegateType, noop.MakeGenericMethod(t));
                register.MakeGenericMethod(t).Invoke(service, new object[] { handler });
                n++;
            }
            catch (Exception ex)
            {
                Logger.Log($"[SimAiPartner] no-op handler for {t.Name}: {ex.Message}");
            }
        }
        Logger.Log($"[SimAiPartner] listening (no-op) to {n} message types.");
    }

    private static void Noop<T>(T message, ulong senderId) where T : INetMessage
    {
    }
}
