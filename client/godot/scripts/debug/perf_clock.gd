extends RefCounted
class_name PerfClock


static func now_us() -> int:
	if OS.has_feature("web"):
		return Time.get_ticks_msec() * 1000
	var usec := Time.get_ticks_usec()
	if usec > 0:
		return usec
	return Time.get_ticks_msec() * 1000


static func elapsed_us(start_us: int) -> int:
	return maxi(0, now_us() - start_us)
