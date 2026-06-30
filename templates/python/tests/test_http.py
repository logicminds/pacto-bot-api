"""HTTP-related tests for {{bot_id}}."""

from __future__ import annotations

{% if http %}import httpx
import pytest
import respx

from {{bot_id_snake}} import bot


class FakeEvent:
    event_id = "test-event-id-123"
    content = ""


@respx.mock
@pytest.mark.asyncio
async def test_http_command_uses_external_api():
    """Example: mock an external API and verify the handler uses it.

    Replace this with a real test for a command that calls an HTTP service.
    """
    route = respx.get("https://api.example.com/health").mock(
        return_value=httpx.Response(200, json={"status": "ok"})
    )

    # TODO: wire this to a real command that uses httpx.
    async with httpx.AsyncClient() as client:
        response = await client.get("https://api.example.com/health")

    assert response.status_code == 200
    assert route.called
{% endif %}
{% if no_http %}# This bot was scaffolded without --http. Add httpx/respx to pyproject.toml
# and implement HTTP tests here if the bot calls external APIs.
{% endif %}
