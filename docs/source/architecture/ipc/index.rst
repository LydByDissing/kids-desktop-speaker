IPC Protocol
============

The services on the device talk to each other over Unix domain sockets in
``/run/kds``. The API Server is the client. The other services are servers.
This page is the contract both sides follow (IPC_REQ_001 to IPC_REQ_004,
DEC-027).

Sockets
-------

.. list-table::
   :header-rows: 1
   :widths: 30 30 40

   * - Service
     - Socket
     - Commands
   * - Audio Player
     - ``/run/kds/audio.sock``
     - ``status``, ``mute``
   * - OTA Updater
     - ``/run/kds/ota.sock``
     - ``status``, ``install``
   * - Bluetooth Service
     - ``/run/kds/bt.sock``
     - ``status``
   * - TTS Engine
     - ``/run/kds/tts.sock``
     - Not yet implemented

A server creates ``/run/kds`` if it doesn't exist, and replaces a stale socket
left by a previous run.

Framing
-------

Each message is one JSON object on one line, encoded as UTF-8 and ended by
``\n`` (newline-delimited JSON). JSON escapes newlines inside strings, so a
newline always ends a message. A message longer than 64 KiB is rejected.

The connection carries one request and one response, then closes. This keeps
servers stateless and makes a failed service easy to detect: the next request
simply can't connect.

Requests
--------

A request names its command in ``cmd``. Other fields are the command's
arguments.

.. code-block:: json

   {"cmd": "install", "url": "https://.../image.raucb", "sha256": "ab12..."}

Responses
---------

Every response has ``ok``. A successful response may add data fields.

.. code-block:: json

   {"ok": true, "state": "idle", "version": "v0.1.0"}

A failed response has ``ok: false`` and a human-readable ``error``.

.. code-block:: json

   {"ok": false, "error": "unknown command: reboot"}

A server answers with an error, rather than closing the connection, when the
request isn't valid JSON, is too long, has no ``cmd``, names an unknown
command, or has invalid arguments.

Timeouts and failures
---------------------

The client allows 2 seconds to connect and 5 seconds for the whole exchange.
If a service isn't running, its socket is missing or refuses connections, and
the API Server answers the HTTP request with ``503 Service Unavailable``. There
is no persistent connection to re-establish: the next request tries again.

Events
------

Servers can't push events yet. Live updates for the web interface (KDS-izr)
will need a subscription command that keeps the connection open, which will be
added to this page when it's built.

Implementation
--------------

The ``kds-ipc`` crate implements both sides: ``kds_ipc::serve`` for servers
and ``kds_ipc::request`` for clients. Services only map commands to responses.
For debugging on a device:

.. code-block:: bash

   echo '{"cmd":"status"}' | socat - UNIX-CONNECT:/run/kds/ota.sock
