#!/bin/bash
set -e

echo "=== IRC MCP Server Manual Test ==="
echo

# Check if server is running
if ! curl -s http://127.0.0.1:5001/mcp > /dev/null 2>&1; then
    echo "Error: IRC MCP server not running at http://127.0.0.1:5001"
    echo "Start it with: cargo run -- --config irc-mcp-config.yaml"
    exit 1
fi

echo "✓ Server is running"
echo

echo "Test 1: Initialize"
curl -s -X POST http://127.0.0.1:5001/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize"}' | jq '.'
echo

echo "Test 2: List Tools"
curl -s -X POST http://127.0.0.1:5001/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | jq '.result.tools[] | .name'
echo

echo "Test 3: Get Status (should show disconnected)"
curl -s -X POST http://127.0.0.1:5001/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"irc_status","arguments":{}}}' | jq '.'
echo

echo "=== Manual tests complete ==="
