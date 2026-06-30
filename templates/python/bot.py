"""Bot handler for {{bot_id}}."""

from __future__ import annotations

from pacto_bot_api import Bot, parse_command

bot = Bot(bot_id="{{bot_id}}")


def _command_args(event) -> list[str]:
    """Return the positional arguments passed after the command name.

    Example: `/price btc` -> `['btc']`; `/hello` -> `[]`.
    """
    parsed = parse_command(event.content)
    if not parsed:
        return []
    return parsed.get("args") or []


{% for command in commands %}
@bot.command("/{{command}}")
async def {{command}}_handler(event, bot):
    """TODO: implement /{{command}}.

    Use `_command_args(event)` to read positional sub-arguments. For example,
    if this command supports sub-types like `/{{command}} foo`, you can
    dispatch like:

        args = _command_args(event)
        subcommand = args[0].lower() if args else None
        if subcommand == "foo":
            ...
    """
    return {
        "event_id": event.event_id,
        "action": "reply",
        "content": "{{command}} placeholder response",
    }

{% endfor %}
@bot.default
async def unknown(event, bot):
    return {"event_id": event.event_id, "action": "ignore"}


if __name__ == "__main__":
    bot.run()
