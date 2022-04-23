#include "hlsl_box.hlsli"

float dist(float3 pos, float size) {
    return length(pos) - size;
}

float3 normal(float3 pos, float size) {
    const float ep = 0.0001;
    return normalize(float3(
        dist(pos, size) - dist(float3(pos.x - ep, pos.y, pos.z), size),
        dist(pos, size) - dist(float3(pos.x, pos.y - ep, pos.z), size),
        dist(pos, size) - dist(float3(pos.x, pos.y, pos.z - ep), size)
    ));
}

float4 main(Input input): SV_Target {
    const float2 pos = normalized_position(input.coord);
    const float3 camera = float3(0.0, 0.0, 10.0);
    const float3 ray = normalize(float3(pos, 0.0) - camera);
    const float2 mouse_pos = normalized_mouse_position();
    const float3 light_dir = normalize(float3(mouse_pos, 1.0));
    float3 cur = camera;
    float3 col = float3(0.0, 0.0, 0.0);
    float size = 0.5;
    for(int i = 0; i < 32; ++i) {
        float d = dist(cur, size);
        if(d < 0.0001) {
            float3 n = normal(cur, size);
            float diff = dot(n, light_dir);
            col = saturate(float3(diff, diff, diff) + float3(0.1, 0.1, 0.1));
            break;
        }
        cur += ray * d;
    }
    return float4(col, 1.0);
}
