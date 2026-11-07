/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Protocol framing from Pavel Herrmann's ek-loop-connect; hardened 2026-11-07. */
#ifndef EKLOCO_RESPONSE_VALIDATION_H
#define EKLOCO_RESPONSE_VALIDATION_H
#ifdef __KERNEL__
#include <linux/types.h>
#include <linux/string.h>
#else
#include <stdbool.h>
#include <string.h>
#endif

static inline bool ekloco_valid_response(const unsigned char *data, int size,
                                        bool write_request, bool sensor_request)
{
	static const unsigned char ack[] = {
		0x10, 0x12, 0x06, 0x00, 0x00, 0x00,
		0x00, 0xaa, 0xbb, 0x65, 0xed
	};
	static const unsigned char header[] = {
		0x10, 0x12, 0x27, 0xaa, 0x01, 0x03, 0x00, 0x20
	};
	bool sensors;

	if (size != 63)
		return false;
	if (write_request)
		return !memcmp(data, ack, sizeof(ack));
	if (memcmp(data, header, sizeof(header)) || data[40] != 0xaa ||
	    data[41] != 0xbb || data[43] != 0xed)
		return false;

	/* Byte 9 is not a reliable fan discriminator: real firmware uses 3.
	 * Sensor replies contain a complete marker before each temperature.
	 */
	sensors = data[8] == 0 && data[9] == 1 && data[10] == 0 &&
		data[12] == 0 && data[13] == 1 && data[14] == 0 &&
		data[16] == 0 && data[17] == 1 && data[18] == 0;
	return sensor_request ? sensors : !sensors && data[21] <= 100;
}
#endif
