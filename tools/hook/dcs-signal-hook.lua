--[[
    dcs-signal-hook.lua

    Starts the DCS Signal Converter daemon when a mission begins, so the panels come to life
    without the user having to remember anything.

    Install to:  Saved Games/DCS/Scripts/Hooks/dcs-signal-hook.lua

    The hook only ever starts the daemon. It never stops it, because a hook
    cannot run when DCS is killed or crashes, and the panels latch: whatever was
    last written stays lit until something writes again. Relying on a shutdown
    message would mean the one case that most needs handling is the one case
    that cannot be.

    Instead the daemon watches the DCS-BIOS export stream itself. A quiet stream
    means the cockpit is gone, so it clears the panels; it only exits once DCS
    itself has gone, which it checks for directly. Sitting in the menu between
    missions is silent but alive, and the daemon stays up through it.

    That is why the daemon is started once per DCS session rather than once per
    mission: it outlives a mission, and launching a second one would put two
    processes on the same panels.

    That flag only protects within one DCS session, though. A crash takes this
    hook down with it, so restarting DCS gives a fresh Lua state that has no
    memory of having started anything. The daemon itself refuses to start a
    second time while one is already running, which is what actually makes that
    case safe.

    Everything is wrapped in pcall. A failure here must never affect DCS.
--]]

-- DSC_DIR is substituted at install time so this hook points at the folder
-- the daemon was installed into. Running the file straight from the repository
-- leaves the placeholder in place and the daemon simply is not found.
local INSTALL_DIR = [[DSC_DIR]]

-- Seconds of export-stream silence after which the daemon clears the panels.
-- Long enough to ride out a slow mission load, short enough that the panels do
-- not sit lit for long after a mission ends.
local IDLE_CLEAR = 20

local Hook = {}
local started = false

-- Capture DCS's global log table before defining the local helper, so the local
-- name cannot shadow it.
local dcsLog = log

local function logInfo(msg)
    pcall(function()
        dcsLog.write('DCS-SIGNAL', dcsLog.INFO, msg)
    end)
end

local function startDaemon()
    if started then return end
    started = true
    pcall(function()
        -- [[...]] literals: no escape processing, so backslashes are safe here.
        local vbs = INSTALL_DIR .. [[\run-hidden.vbs]]
        local f = io.open(vbs, 'r')
        if not f then
            logInfo('daemon not installed at ' .. vbs .. ' - skipping')
            return
        end
        f:close()
        -- wscript with a hidden window. os.execute would otherwise flash a
        -- console on every mission start.
        os.execute('start "" /B wscript.exe "' .. vbs .. '" ' .. IDLE_CLEAR)
        logInfo('daemon launched, clearing panels after ' .. IDLE_CLEAR .. 's of silence')
    end)
end

function Hook.onSimulationStart()
    pcall(startDaemon)
end

-- `started` is deliberately not reset here. The daemon survives a mission
-- ending, so the next mission start must not launch a second one.
function Hook.onSimulationStop()
    pcall(function()
        logInfo('mission stop; the daemon will clear the panels and wait')
    end)
end

DCS.setUserCallbacks(Hook)
