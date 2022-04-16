struct VSInput {
	float3 position: POSITION;
	float2 uv: TEXCOORD0;
};

struct VSOutput {
	float4 position: SV_Position;
	float2 uv: TEXCOORD0;
};

Texture2D tex: register(t0);
SamplerState samp: register(s0);

VSOutput vs_main(VSInput input) {
	VSOutput output;
	output.position = float4(input.position, 1.0);
	output.uv = input.uv;
	return output;
}

float4 ps_main(VSOutput vs): SV_Target {
	return tex.Sample(samp, vs.uv);
}