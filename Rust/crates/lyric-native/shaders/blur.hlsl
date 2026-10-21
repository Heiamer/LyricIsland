// Adapted from liquidDX11, Copyright (c) 2026 poncipp (https://github.com/Pondot).
// MIT License; see liquidDX11-LICENSE.txt. Source: https://github.com/VMonoShiki/ImGui

cbuffer BlurCB : register(b0)
{
    float2 texel;
    float  spread;
    float  _pad;
};

Texture2D    Src        : register(t0);
SamplerState LinearClamp : register(s0);

struct VSO { float4 pos : SV_Position; float2 uv : TEXCOORD0; };

VSO VSFull(uint id : SV_VertexID)
{
    VSO o;
    float2 uv = float2((id << 1) & 2, id & 2);
    o.uv  = uv;
    o.pos = float4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    return o;
}

float4 PSDown(VSO i) : SV_Target
{
    float2 h = texel * (0.5 * spread);
    float4 s = Src.Sample(LinearClamp, i.uv) * 4.0;
    s += Src.Sample(LinearClamp, i.uv + float2(-h.x, -h.y));
    s += Src.Sample(LinearClamp, i.uv + float2( h.x, -h.y));
    s += Src.Sample(LinearClamp, i.uv + float2(-h.x,  h.y));
    s += Src.Sample(LinearClamp, i.uv + float2( h.x,  h.y));
    return s / 8.0;
}

float4 PSUp(VSO i) : SV_Target
{
    float2 h = texel * (0.5 * spread);
    float4 s = Src.Sample(LinearClamp, i.uv + float2(-h.x * 2.0, 0.0));
    s += Src.Sample(LinearClamp, i.uv + float2(-h.x,  h.y)) * 2.0;
    s += Src.Sample(LinearClamp, i.uv + float2( 0.0,  h.y * 2.0));
    s += Src.Sample(LinearClamp, i.uv + float2( h.x,  h.y)) * 2.0;
    s += Src.Sample(LinearClamp, i.uv + float2( h.x * 2.0, 0.0));
    s += Src.Sample(LinearClamp, i.uv + float2( h.x, -h.y)) * 2.0;
    s += Src.Sample(LinearClamp, i.uv + float2( 0.0, -h.y * 2.0));
    s += Src.Sample(LinearClamp, i.uv + float2(-h.x, -h.y)) * 2.0;
    return s / 12.0;
}
