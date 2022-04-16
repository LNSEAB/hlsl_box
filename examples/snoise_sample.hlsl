#include "hlsl_box.hlsl"
#include "snoise.hlsl"

float4 main(Input input): SV_Target {
    float2 st = normalized_position(input.coord);
    st.x += HLSLBox.time;
    st *= 10.0;
    float color = snoise(st) * 0.5 + 0.5;
    return float4(color, color, color, 1.0);
}