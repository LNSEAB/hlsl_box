#include "hlsl_box.hlsli"

float circle(float2 pos, float size) {
    return length(pos) < size ? 1.0 : 0.0;
}

float4 main(Input input): SV_TARGET {
    const float2 pos = normalized_position(input.coord);
    const float d = circle(pos, 0.5);
    return float4(d, d, d, 1.0);
}