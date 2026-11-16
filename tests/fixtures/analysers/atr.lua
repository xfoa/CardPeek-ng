-- CardPeek-NG ATR analyser.
-- @id atr
-- @title ATR
-- @description Inspect the current Answer to Reset
-- @version 1.0.0

local cardpeek = require("cardpeek")
local root = cardpeek.nodes.root()
local card = root:append("card", "Synthetic card")
local atr_node = card:append("atr", "Answer to Reset")
local atr, failure = cardpeek.card.atr()
assert(failure == nil, failure and failure.message or "ATR failed")
atr_node:set("value", atr.bytes)
if atr.record ~= nil then
    atr_node:set_provenance(atr.record, "Cold reset")
end
cardpeek.log.info("Read " .. tostring(#atr.bytes) .. " ATR bytes")
