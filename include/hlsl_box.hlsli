struct Input {
    float4 position: SV_Position;
    float2 coord: TEXCOORD0; // left-top 0 ..= rendering resolution
};

struct Parameters {
    float2 resolution;
    float2 mouse; // left-top 0.0 ..= 1.0
    float time;
};

ConstantBuffer<Parameters> HLSLBox: register(b0);

float2 normalized_position(float2 coord) {
    return float2(coord.x * 2.0 - HLSLBox.resolution.x, HLSLBox.resolution.y - coord.y * 2.0)
        / min(HLSLBox.resolution.x, HLSLBox.resolution.y);
}

float2 normalized_mouse_position() {
    return float2(HLSLBox.mouse.x * 2.0 - 1.0, 1.0 - HLSLBox.mouse.y * 2.0);
}