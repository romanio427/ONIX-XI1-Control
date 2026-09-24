Name:           onix-xi1-control
Version:        %{app_version}
Release:        1%{?dist}
Summary:        Hardware control for the ONIX Alpha XI1 USB DAC
License:        MIT
URL:            https://github.com/romanio427/ONIX-XI1-Control
BuildArch:      x86_64

%description
Controls volume and hardware settings of the ONIX Alpha XI1 over USB.

%install
install -Dm755 %{source_dir}/onix-xi1-pc %{buildroot}/usr/bin/onix-xi1-pc
install -Dm644 %{project_dir}/native/packaging/70-onix-xi1.rules %{buildroot}/usr/lib/udev/rules.d/70-onix-xi1.rules
install -Dm644 %{project_dir}/native/packaging/onix-xi1-control.desktop %{buildroot}/usr/share/applications/io.github.onix-xi1-control.desktop
install -Dm644 %{project_dir}/native/packaging/io.github.onix-xi1-control.autostart.desktop %{buildroot}/etc/xdg/autostart/io.github.onix-xi1-control.desktop
install -Dm644 %{project_dir}/native/ui/icon.svg %{buildroot}/usr/share/icons/hicolor/scalable/apps/io.github.onix-xi1-control.svg
install -Dm644 %{source_dir}/LICENSE %{buildroot}/usr/share/licenses/%{name}/LICENSE
install -Dm644 %{source_dir}/THIRD_PARTY_NOTICES.md %{buildroot}/usr/share/doc/%{name}/THIRD_PARTY_NOTICES.md
install -Dm644 %{source_dir}/THIRD-PARTY-LICENSES.html %{buildroot}/usr/share/doc/%{name}/THIRD-PARTY-LICENSES.html
install -Dm644 %{source_dir}/SLINT-LICENSE.md %{buildroot}/usr/share/doc/%{name}/SLINT-LICENSE.md

%post
udevadm control --reload-rules >/dev/null 2>&1 || :
udevadm trigger --subsystem-match=usb --attr-match=idVendor=26b6 --attr-match=idProduct=60c0 >/dev/null 2>&1 || :

%postun
udevadm control --reload-rules >/dev/null 2>&1 || :

%files
/usr/bin/onix-xi1-pc
/usr/lib/udev/rules.d/70-onix-xi1.rules
/usr/share/applications/io.github.onix-xi1-control.desktop
/etc/xdg/autostart/io.github.onix-xi1-control.desktop
/usr/share/icons/hicolor/scalable/apps/io.github.onix-xi1-control.svg
%license /usr/share/licenses/%{name}/LICENSE
%doc /usr/share/doc/%{name}/THIRD_PARTY_NOTICES.md
%doc /usr/share/doc/%{name}/THIRD-PARTY-LICENSES.html
%doc /usr/share/doc/%{name}/SLINT-LICENSE.md

%changelog
* Sat Sep 05 2026 romanio427 - 1.0.0-1
- Initial package
