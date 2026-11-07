/* SPDX-License-Identifier: GPL-2.0-or-later */
#include <assert.h>
#include <stdio.h>
#include "response-validation.h"

int main(void)
{
	/* Actual F1 response captured from hardware on 2026-11-07. */
	const char *hex = "101227aa010300200003000000000000000000000018000000000000000000000000000000000000aabb4eed00000000000000000000000000000000000000";
	unsigned char fan[63], sensors[63], ack[63] = {
		0x10, 0x12, 0x06, 0, 0, 0, 0, 0xaa, 0xbb, 0x65, 0xed
	};
	unsigned int byte;
	int i;

	assert(strlen(hex) == 126);
	for (i = 0; i < 63; i++) {
		assert(sscanf(hex + i * 2, "%2x", &byte) == 1);
		fan[i] = byte;
	}
	assert(ekloco_valid_response(fan, 63, false, false));
	assert(!ekloco_valid_response(fan, 63, false, true));
	assert(!ekloco_valid_response(fan, 62, false, false));
	assert(!ekloco_valid_response(fan, 64, false, false));
	fan[9] = 0; /* F6 uses zero, while F1-F5 use three. */
	assert(ekloco_valid_response(fan, 63, false, false));
	fan[21] = 101;
	assert(!ekloco_valid_response(fan, 63, false, false));
	memcpy(sensors, fan, 63);
	for (i = 8; i <= 16; i += 4) {
		sensors[i] = 0; sensors[i + 1] = 1; sensors[i + 2] = 0;
		sensors[i + 3] = 0xe7;
	}
	sensors[21] = 0;
	assert(ekloco_valid_response(sensors, 63, false, true));
	assert(!ekloco_valid_response(sensors, 63, false, false));
	sensors[43] = 0;
	assert(!ekloco_valid_response(sensors, 63, false, true));
	assert(ekloco_valid_response(ack, 63, true, false));
	assert(!ekloco_valid_response(ack, 63, false, false));
	ack[7] = 0;
	assert(!ekloco_valid_response(ack, 63, true, false));
	puts("response validation: PASS (hardware F1, F6 variant, sensors, malformed reports, ACK)");
	return 0;
}
