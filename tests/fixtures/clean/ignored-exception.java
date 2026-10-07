final class Permissions {
	static boolean isDeclared(PackageManager manager, String permission) {
		try {
			return manager.getPermissionInfo(permission, 0) != null;
		} catch (PackageManager.NameNotFoundException ignored) {
		}
		return false;
	}
}
