"""wx-tools quickstart — run with: python quickstart.py"""

from wx_tools import WxClient

wx = WxClient("http://localhost:8080")

# Check server health
print("Server:", wx.health())

# Current conditions in Oklahoma City
print("\n=== Conditions at OKC ===")
conditions = wx.conditions(35.22, -97.44)
print(conditions)

# METAR
print("\n=== METAR KOKC ===")
metar = wx.metar("KOKC")
print(metar)

# Scan for highest CAPE
print("\n=== Top 5 CAPE hotspots ===")
cape = wx.scan("cape", mode="max", top_n=5)
print(cape)

# Tile URL for mapping libraries
print("\n=== Tile URLs ===")
print("CAPE:", wx.cape_tile_url())
print("Radar:", wx.radar_tile_url())
print("Temp:", wx.temp_tile_url())

# Folium example (uncomment if folium installed):
# import folium
# m = folium.Map(location=[39, -98], zoom_start=5, tiles="cartodbdark_matter")
# folium.TileLayer(tiles=wx.cape_tile_url(), attr="wx-tools", opacity=0.7).add_to(m)
# m.save("cape_map.html")
