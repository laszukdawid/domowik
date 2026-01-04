"""
Email notification system for new matching listings.

Sends digest emails after scraper runs.
"""

import smtplib
from email.mime.text import MIMEText
from email.mime.multipart import MIMEMultipart
from datetime import datetime
from typing import Any

from jinja2 import Template
from sqlalchemy import select, and_
from sqlalchemy.ext.asyncio import AsyncSession

from app.config import settings
from app.models import User, UserPreferences, Listing, AmenityScore


EMAIL_TEMPLATE = """
<!DOCTYPE html>
<html>
<head>
    <style>
        body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; margin: 0; padding: 20px; background: #f5f5f5; }
        .container { max-width: 600px; margin: 0 auto; background: white; border-radius: 8px; overflow: hidden; }
        .header { background: #2563eb; color: white; padding: 20px; text-align: center; }
        .content { padding: 20px; }
        .listing { border: 1px solid #e5e7eb; border-radius: 8px; padding: 16px; margin-bottom: 16px; }
        .listing-header { display: flex; justify-content: space-between; align-items: start; }
        .address { font-weight: 600; font-size: 16px; color: #111827; }
        .price { font-size: 18px; font-weight: 700; color: #059669; }
        .details { color: #6b7280; font-size: 14px; margin-top: 8px; }
        .amenities { background: #f9fafb; padding: 12px; border-radius: 6px; margin-top: 12px; font-size: 13px; }
        .score { display: inline-block; background: #dcfce7; color: #166534; padding: 4px 8px; border-radius: 4px; font-weight: 600; font-size: 12px; }
        .view-btn { display: inline-block; background: #2563eb; color: white; padding: 8px 16px; border-radius: 6px; text-decoration: none; font-size: 14px; margin-top: 12px; }
        .footer { background: #f9fafb; padding: 16px; text-align: center; font-size: 12px; color: #6b7280; }
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1 style="margin: 0;">HomeHero</h1>
            <p style="margin: 8px 0 0; opacity: 0.9;">{{ count }} new listing{{ 's' if count != 1 else '' }} match your search</p>
        </div>
        <div class="content">
            {% for listing in listings %}
            <div class="listing">
                <div class="listing-header">
                    <div class="address">{{ listing.address }}</div>
                    <div class="price">${{ "{:,}".format(listing.price) }}</div>
                </div>
                <div class="details">
                    {{ listing.bedrooms or '?' }} bed · {{ listing.bathrooms or '?' }} bath
                    {% if listing.sqft %} · {{ "{:,}".format(listing.sqft) }} sqft{% endif %}
                    {% if listing.property_type %} · {{ listing.property_type }}{% endif %}
                </div>
                {% if listing.amenity %}
                <div class="amenities">
                    {% if listing.amenity.nearest_park_m %}🌳 Park: {{ listing.amenity.nearest_park_m }}m{% endif %}
                    {% if listing.amenity.nearest_coffee_m %} · ☕ Coffee: {{ listing.amenity.nearest_coffee_m }}m{% endif %}
                    {% if listing.amenity.walkability_score %}
                    <br><span class="score">Walkability: {{ listing.amenity.walkability_score }}/100</span>
                    {% endif %}
                </div>
                {% endif %}
                <a href="{{ listing.app_url }}" class="view-btn">View Details →</a>
            </div>
            {% endfor %}
        </div>
        <div class="footer">
            <p>You're receiving this because you have email notifications enabled.</p>
            <p>Update your preferences at <a href="{{ app_base_url }}">HomeHero</a></p>
        </div>
    </div>
</body>
</html>
"""


def matches_preferences(listing: Listing, prefs: UserPreferences) -> bool:
    """Check if a listing matches user preferences."""
    if prefs.min_price and listing.price < prefs.min_price:
        return False
    if prefs.max_price and listing.price > prefs.max_price:
        return False
    if prefs.min_bedrooms and (listing.bedrooms or 0) < prefs.min_bedrooms:
        return False
    if prefs.min_sqft and (listing.sqft or 0) < prefs.min_sqft:
        return False
    if prefs.cities and listing.city not in prefs.cities:
        return False
    if prefs.property_types and listing.property_type not in prefs.property_types:
        return False

    # Check amenity requirements
    if prefs.max_park_distance:
        # If user requires park within X meters, listing must have amenity data
        if not listing.amenity_score:
            return False
        if (
            listing.amenity_score.nearest_park_m is None
            or listing.amenity_score.nearest_park_m > prefs.max_park_distance
        ):
            return False

    return True


def send_email(to_email: str, subject: str, html_content: str):
    """Send an email via SMTP."""
    if not settings.smtp_pass:
        print(f"SMTP not configured, skipping email to {to_email}")
        return

    msg = MIMEMultipart("alternative")
    msg["Subject"] = subject
    msg["From"] = settings.smtp_user
    msg["To"] = to_email

    msg.attach(MIMEText(html_content, "html"))

    try:
        with smtplib.SMTP(settings.smtp_host, settings.smtp_port) as server:
            server.starttls()
            server.login(settings.smtp_user, settings.smtp_pass)
            server.sendmail(settings.smtp_user, to_email, msg.as_string())
        print(f"Email sent to {to_email}")
    except Exception as e:
        print(f"Failed to send email to {to_email}: {e}")


async def send_notifications(
    session: AsyncSession, new_listings: list[Listing], app_base_url: str = "https://homehero.pro"
):
    """Send email notifications to users for matching new listings."""
    if not new_listings:
        print("No new listings to notify about")
        return

    # Get all users with email notifications enabled
    result = await session.execute(
        select(User, UserPreferences)
        .join(UserPreferences)
        .where(UserPreferences.notify_email == True)  # noqa: E712
    )
    users_with_prefs = result.all()

    print(f"Checking notifications for {len(users_with_prefs)} users")

    template = Template(EMAIL_TEMPLATE)

    for user, prefs in users_with_prefs:
        # Filter listings that match this user's preferences
        matching = [l for l in new_listings if matches_preferences(l, prefs)]

        if not matching:
            continue

        print(f"Sending {len(matching)} listings to {user.email}")

        # Prepare listing data for template
        listings_data = []
        for listing in matching[:10]:  # Limit to 10 per email
            data: dict[str, Any] = {
                "address": listing.address,
                "price": listing.price,
                "bedrooms": listing.bedrooms,
                "bathrooms": listing.bathrooms,
                "sqft": listing.sqft,
                "property_type": listing.property_type,
                "app_url": f"{app_base_url}/?listing={listing.id}",
                "amenity": None,
            }
            if listing.amenity_score:
                data["amenity"] = {
                    "nearest_park_m": listing.amenity_score.nearest_park_m,
                    "nearest_coffee_m": listing.amenity_score.nearest_coffee_m,
                    "walkability_score": listing.amenity_score.walkability_score,
                }
            listings_data.append(data)

        html = template.render(
            listings=listings_data,
            count=len(matching),
            app_base_url=app_base_url,
        )

        subject = f"🏠 {len(matching)} new listing{'s' if len(matching) != 1 else ''} match your search"
        send_email(user.email, subject, html)
