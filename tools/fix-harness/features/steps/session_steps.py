import os
import tempfile
import threading

import quickfix as fix
from behave import given, when, then

FIX_PORT = int(os.environ.get("FIX_PORT", "9878"))


class HarnessApp(fix.Application):
    def __init__(self):
        super().__init__()
        self.logon_event = threading.Event()
        self.session_id = None

    def onCreate(self, sessionID):
        pass

    def onLogon(self, sessionID):
        self.session_id = sessionID
        self.logon_event.set()

    def onLogout(self, sessionID):
        self.logon_event.clear()

    def toAdmin(self, message, sessionID):
        pass

    def fromAdmin(self, message, sessionID):
        pass

    def toApp(self, message, sessionID):
        pass

    def fromApp(self, message, sessionID):
        pass


def _make_cfg(sender, target, port):
    cfg = (
        "[DEFAULT]\n"
        "ConnectionType=initiator\n"
        "HeartBtInt=30\n"
        f"SenderCompID={sender}\n"
        f"TargetCompID={target}\n"
        "ResetOnLogon=Y\n"
        "ResetOnDisconnect=Y\n"
        "\n"
        "[SESSION]\n"
        "BeginString=FIX.4.4\n"
        "SocketConnectHost=127.0.0.1\n"
        f"SocketConnectPort={port}\n"
    )
    f = tempfile.NamedTemporaryFile(mode="w", suffix=".cfg", delete=False)
    f.write(cfg)
    f.close()
    return f.name


@given("a FIX 4.4 session with sender {sender} and target {target}")
def step_session_config(context, sender, target):
    context.sender = sender
    context.target = target


@when("the harness connects and sends Logon")
def step_send_logon(context):
    cfg_path = _make_cfg(context.sender, context.target, FIX_PORT)
    settings = fix.SessionSettings(cfg_path)
    context.app = HarnessApp()
    store = fix.MemoryStoreFactory(settings)
    log = fix.ScreenLogFactory(settings)
    context.initiator = fix.SocketInitiator(context.app, store, log, settings)
    context.initiator.start()


@then("the engine replies with Logon")
def step_logon_reply(context):
    assert context.app.logon_event.wait(timeout=10), \
        "engine did not reply with Logon within 10s"


@then("the session is active")
def step_session_active(context):
    assert context.app.session_id is not None
